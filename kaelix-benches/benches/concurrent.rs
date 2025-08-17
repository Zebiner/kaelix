//! Concurrent performance benchmarks for MemoryStreamer.
//!
//! This benchmark suite validates performance under concurrent load including
//! multi-producer single-consumer, single-producer multi-consumer,
//! multi-producer multi-consumer scenarios, and lock-free data structure performance.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use kaelix_benches::prelude::*;
use kaelix_benches::{BenchmarkConfig, PerformanceValidator};
use kaelix_core::{Message, Result};
use bytes::Bytes;
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, RwLock, Mutex};

/// Concurrency levels for testing.
const CONCURRENCY_LEVELS: &[usize] = &[1, 2, 4, 8, 16, 32, 64];

/// Message counts per task.
const MESSAGES_PER_TASK: &[usize] = &[100, 1000, 10000];

/// Queue sizes for channel testing.
const QUEUE_SIZES: &[usize] = &[100, 1000, 10000, 100000];

/// Benchmark multi-producer single-consumer scenario.
fn bench_multi_producer_single_consumer(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("multi_producer_single_consumer");
    
    for &producer_count in &[2, 4, 8, 16, 32] {
        for &messages_per_producer in &[1000, 10000] {
            let total_messages = producer_count * messages_per_producer;
            group.throughput(Throughput::Elements(total_messages as u64));
            
            group.bench_with_input(
                BenchmarkId::new(format!("{}p_{}m", producer_count, messages_per_producer), producer_count),
                &(producer_count, messages_per_producer),
                |b, &(producers, messages)| {
                    b.to_async(&rt).iter(|| async {
                        let ctx = BenchmarkContext::concurrent().await;
                        let consumer = ctx.create_consumer("mpsc_consumer").await.unwrap();
                        
                        // Start consumer task
                        let consumer_handle = {
                            let expected_messages = producers * messages;
                            tokio::spawn(async move {
                                let mut received = 0;
                                let start = tokio::time::Instant::now();
                                
                                while received < expected_messages {
                                    match consumer.poll_batch(Duration::from_millis(10)).await {
                                        Ok(batch) => received += batch.len(),
                                        Err(_) => tokio::time::sleep(Duration::from_millis(1)).await,
                                    }
                                }
                                
                                start.elapsed()
                            })
                        };
                        
                        // Start producer tasks
                        let mut producer_handles = Vec::new();
                        for producer_id in 0..producers {
                            let ctx_clone = ctx.clone();
                            let handle = tokio::spawn(async move {
                                let start = tokio::time::Instant::now();
                                
                                for i in 0..messages {
                                    let payload = format!("Producer {} Message {}", producer_id, i);
                                    let message = Message::new("mpsc-test", Bytes::from(payload)).unwrap();
                                    ctx_clone.publisher.publish(message).await.unwrap();
                                }
                                
                                start.elapsed()
                            });
                            producer_handles.push(handle);
                        }
                        
                        // Wait for all tasks to complete
                        let producer_times = futures::future::join_all(producer_handles).await;
                        let consumer_time = consumer_handle.await.unwrap();
                        
                        criterion::black_box((producer_times, consumer_time))
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark single-producer multi-consumer scenario.
fn bench_single_producer_multi_consumer(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("single_producer_multi_consumer");
    
    for &consumer_count in &[2, 4, 8, 16, 32] {
        for &total_messages in &[10000, 100000] {
            group.throughput(Throughput::Elements(total_messages as u64));
            
            group.bench_with_input(
                BenchmarkId::new(format!("{}c_{}m", consumer_count, total_messages), consumer_count),
                &(consumer_count, total_messages),
                |b, &(consumers, messages)| {
                    b.to_async(&rt).iter(|| async {
                        let ctx = BenchmarkContext::concurrent().await;
                        
                        // Start consumer tasks
                        let mut consumer_handles = Vec::new();
                        let messages_per_consumer = messages / consumers;
                        
                        for consumer_id in 0..consumers {
                            let consumer = ctx.create_consumer(&format!("spmc_consumer_{}", consumer_id)).await.unwrap();
                            let handle = tokio::spawn(async move {
                                let mut received = 0;
                                let start = tokio::time::Instant::now();
                                
                                while received < messages_per_consumer {
                                    match consumer.poll_batch(Duration::from_millis(10)).await {
                                        Ok(batch) => received += batch.len(),
                                        Err(_) => tokio::time::sleep(Duration::from_millis(1)).await,
                                    }
                                }
                                
                                (start.elapsed(), received)
                            });
                            consumer_handles.push(handle);
                        }
                        
                        // Start producer task
                        let producer_handle = {
                            let ctx_clone = ctx.clone();
                            tokio::spawn(async move {
                                let start = tokio::time::Instant::now();
                                
                                for i in 0..messages {
                                    let payload = format!("Message {}", i);
                                    let message = Message::new("spmc-test", Bytes::from(payload)).unwrap();
                                    ctx_clone.publisher.publish(message).await.unwrap();
                                }
                                
                                start.elapsed()
                            })
                        };
                        
                        // Wait for all tasks to complete
                        let consumer_results = futures::future::join_all(consumer_handles).await;
                        let producer_time = producer_handle.await.unwrap();
                        
                        criterion::black_box((consumer_results, producer_time))
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark multi-producer multi-consumer scenario.
fn bench_multi_producer_multi_consumer(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("multi_producer_multi_consumer");
    
    for &task_count in &[4, 8, 16, 32] {
        for &messages_per_task in &[1000, 5000] {
            let total_messages = task_count * messages_per_task * 2; // producers + consumers
            group.throughput(Throughput::Elements(total_messages as u64));
            
            group.bench_with_input(
                BenchmarkId::new(format!("{}t_{}m", task_count, messages_per_task), task_count),
                &(task_count, messages_per_task),
                |b, &(tasks, messages)| {
                    b.to_async(&rt).iter(|| async {
                        let ctx = BenchmarkContext::concurrent().await;
                        let producer_count = tasks;
                        let consumer_count = tasks;
                        
                        // Start consumer tasks
                        let mut consumer_handles = Vec::new();
                        for consumer_id in 0..consumer_count {
                            let consumer = ctx.create_consumer(&format!("mpmc_consumer_{}", consumer_id)).await.unwrap();
                            let handle = tokio::spawn(async move {
                                let mut received = 0;
                                let start = tokio::time::Instant::now();
                                
                                while received < messages {
                                    match consumer.poll_batch(Duration::from_millis(5)).await {
                                        Ok(batch) => received += batch.len(),
                                        Err(_) => tokio::time::sleep(Duration::from_millis(1)).await,
                                    }
                                }
                                
                                (start.elapsed(), received)
                            });
                            consumer_handles.push(handle);
                        }
                        
                        // Start producer tasks
                        let mut producer_handles = Vec::new();
                        for producer_id in 0..producer_count {
                            let ctx_clone = ctx.clone();
                            let handle = tokio::spawn(async move {
                                let start = tokio::time::Instant::now();
                                
                                for i in 0..messages {
                                    let payload = format!("Producer {} Message {}", producer_id, i);
                                    let message = Message::new("mpmc-test", Bytes::from(payload)).unwrap();
                                    ctx_clone.publisher.publish(message).await.unwrap();
                                }
                                
                                start.elapsed()
                            });
                            producer_handles.push(handle);
                        }
                        
                        // Wait for all tasks to complete
                        let consumer_results = futures::future::join_all(consumer_handles).await;
                        let producer_results = futures::future::join_all(producer_handles).await;
                        
                        criterion::black_box((consumer_results, producer_results))
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark lock-free data structure performance.
fn bench_lock_free_structures(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("lock_free_structures");
    
    for &concurrency in &[2, 4, 8, 16] {
        for &operations in &[10000, 100000] {
            // Lock-free queue (crossbeam)
            group.bench_with_input(
                BenchmarkId::new(format!("crossbeam_queue_{}c", concurrency), operations),
                &(concurrency, operations),
                |b, &(threads, ops)| {
                    b.to_async(&rt).iter(|| async {
                        let queue = Arc::new(crossbeam::queue::ArrayQueue::new(ops));
                        let ops_per_thread = ops / threads;
                        
                        // Producer tasks
                        let mut producer_handles = Vec::new();
                        for _ in 0..threads / 2 {
                            let queue_clone = queue.clone();
                            let handle = tokio::spawn(async move {
                                let start = tokio::time::Instant::now();
                                for i in 0..ops_per_thread {
                                    while queue_clone.push(i).is_err() {
                                        tokio::task::yield_now().await;
                                    }
                                }
                                start.elapsed()
                            });
                            producer_handles.push(handle);
                        }
                        
                        // Consumer tasks
                        let mut consumer_handles = Vec::new();
                        for _ in 0..threads / 2 {
                            let queue_clone = queue.clone();
                            let handle = tokio::spawn(async move {
                                let start = tokio::time::Instant::now();
                                let mut consumed = 0;
                                while consumed < ops_per_thread {
                                    if queue_clone.pop().is_ok() {
                                        consumed += 1;
                                    } else {
                                        tokio::task::yield_now().await;
                                    }
                                }
                                start.elapsed()
                            });
                            consumer_handles.push(handle);
                        }
                        
                        let producer_times = futures::future::join_all(producer_handles).await;
                        let consumer_times = futures::future::join_all(consumer_handles).await;
                        
                        criterion::black_box((producer_times, consumer_times))
                    });
                },
            );
            
            // Mutex-based queue for comparison
            group.bench_with_input(
                BenchmarkId::new(format!("mutex_queue_{}c", concurrency), operations),
                &(concurrency, operations),
                |b, &(threads, ops)| {
                    b.to_async(&rt).iter(|| async {
                        let queue = Arc::new(Mutex::new(std::collections::VecDeque::new()));
                        let ops_per_thread = ops / threads;
                        
                        // Producer tasks
                        let mut producer_handles = Vec::new();
                        for _ in 0..threads / 2 {
                            let queue_clone = queue.clone();
                            let handle = tokio::spawn(async move {
                                let start = tokio::time::Instant::now();
                                for i in 0..ops_per_thread {
                                    let mut q = queue_clone.lock().await;
                                    q.push_back(i);
                                }
                                start.elapsed()
                            });
                            producer_handles.push(handle);
                        }
                        
                        // Consumer tasks
                        let mut consumer_handles = Vec::new();
                        for _ in 0..threads / 2 {
                            let queue_clone = queue.clone();
                            let handle = tokio::spawn(async move {
                                let start = tokio::time::Instant::now();
                                let mut consumed = 0;
                                while consumed < ops_per_thread {
                                    let mut q = queue_clone.lock().await;
                                    if q.pop_front().is_some() {
                                        consumed += 1;
                                    }
                                }
                                start.elapsed()
                            });
                            consumer_handles.push(handle);
                        }
                        
                        let producer_times = futures::future::join_all(producer_handles).await;
                        let consumer_times = futures::future::join_all(consumer_handles).await;
                        
                        criterion::black_box((producer_times, consumer_times))
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark async task scheduling overhead.
fn bench_async_task_overhead(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("async_task_overhead");
    
    for &task_count in &[10, 100, 1000, 10000] {
        // Spawn many lightweight tasks
        group.bench_with_input(
            BenchmarkId::new("spawn_tasks", task_count),
            &task_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let start = tokio::time::Instant::now();
                    
                    let mut handles = Vec::with_capacity(count);
                    for i in 0..count {
                        let handle = tokio::spawn(async move {
                            // Minimal work
                            i + 1
                        });
                        handles.push(handle);
                    }
                    
                    let results = futures::future::join_all(handles).await;
                    let elapsed = start.elapsed();
                    
                    criterion::black_box((results, elapsed))
                });
            },
        );
        
        // Task communication overhead
        group.bench_with_input(
            BenchmarkId::new("task_communication", task_count),
            &task_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let (tx, mut rx) = mpsc::channel(count);
                    let start = tokio::time::Instant::now();
                    
                    // Sender task
                    let sender_handle = tokio::spawn(async move {
                        for i in 0..count {
                            tx.send(i).await.unwrap();
                        }
                    });
                    
                    // Receiver task
                    let receiver_handle = tokio::spawn(async move {
                        let mut received = 0;
                        while let Some(_) = rx.recv().await {
                            received += 1;
                            if received >= count {
                                break;
                            }
                        }
                        received
                    });
                    
                    let (_, received) = tokio::join!(sender_handle, receiver_handle);
                    let elapsed = start.elapsed();
                    
                    criterion::black_box((received, elapsed))
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark contention scenarios.
fn bench_contention_scenarios(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("contention_scenarios");
    
    for &thread_count in &[2, 4, 8, 16, 32] {
        // High contention - single shared resource
        group.bench_with_input(
            BenchmarkId::new("high_contention", thread_count),
            &thread_count,
            |b, &threads| {
                b.to_async(&rt).iter(|| async {
                    let shared_counter = Arc::new(Mutex::new(0u64));
                    let operations_per_thread = 10000;
                    
                    let mut handles = Vec::new();
                    for _ in 0..threads {
                        let counter_clone = shared_counter.clone();
                        let handle = tokio::spawn(async move {
                            let start = tokio::time::Instant::now();
                            for _ in 0..operations_per_thread {
                                let mut count = counter_clone.lock().await;
                                *count += 1;
                            }
                            start.elapsed()
                        });
                        handles.push(handle);
                    }
                    
                    let times = futures::future::join_all(handles).await;
                    criterion::black_box(times)
                });
            },
        );
        
        // Low contention - partitioned resources
        group.bench_with_input(
            BenchmarkId::new("low_contention", thread_count),
            &thread_count,
            |b, &threads| {
                b.to_async(&rt).iter(|| async {
                    // Create per-thread counters to reduce contention
                    let counters: Vec<Arc<Mutex<u64>>> = (0..threads)
                        .map(|_| Arc::new(Mutex::new(0)))
                        .collect();
                    let operations_per_thread = 10000;
                    
                    let mut handles = Vec::new();
                    for (i, counter) in counters.iter().enumerate() {
                        let counter_clone = counter.clone();
                        let handle = tokio::spawn(async move {
                            let start = tokio::time::Instant::now();
                            for _ in 0..operations_per_thread {
                                let mut count = counter_clone.lock().await;
                                *count += 1;
                            }
                            start.elapsed()
                        });
                        handles.push(handle);
                    }
                    
                    let times = futures::future::join_all(handles).await;
                    criterion::black_box(times)
                });
            },
        );
        
        // Read-heavy workload with RwLock
        group.bench_with_input(
            BenchmarkId::new("read_heavy", thread_count),
            &thread_count,
            |b, &threads| {
                b.to_async(&rt).iter(|| async {
                    let shared_data = Arc::new(RwLock::new(vec![1u64; 1000]));
                    let operations_per_thread = 10000;
                    
                    let mut handles = Vec::new();
                    for thread_id in 0..threads {
                        let data_clone = shared_data.clone();
                        let handle = tokio::spawn(async move {
                            let start = tokio::time::Instant::now();
                            
                            for i in 0..operations_per_thread {
                                if i % 100 == 0 && thread_id == 0 {
                                    // Occasional write (only from one thread)
                                    let mut data = data_clone.write().await;
                                    data[i % data.len()] = i as u64;
                                } else {
                                    // Frequent reads
                                    let data = data_clone.read().await;
                                    let _sum: u64 = data.iter().sum();
                                }
                            }
                            
                            start.elapsed()
                        });
                        handles.push(handle);
                    }
                    
                    let times = futures::future::join_all(handles).await;
                    criterion::black_box(times)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark work-stealing vs dedicated queues.
fn bench_work_stealing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("work_stealing");
    
    for &worker_count in &[2, 4, 8] {
        let task_count = 10000;
        
        // Dedicated queues (no work stealing)
        group.bench_with_input(
            BenchmarkId::new("dedicated_queues", worker_count),
            &worker_count,
            |b, &workers| {
                b.to_async(&rt).iter(|| async {
                    let tasks_per_worker = task_count / workers;
                    
                    let mut handles = Vec::new();
                    for worker_id in 0..workers {
                        let handle = tokio::spawn(async move {
                            let start = tokio::time::Instant::now();
                            
                            // Simulate work with varying complexity
                            for i in 0..tasks_per_worker {
                                let work_amount = if i % 10 == 0 { 1000 } else { 100 }; // Some tasks take longer
                                
                                // Simulate CPU work
                                let mut sum = 0u64;
                                for j in 0..work_amount {
                                    sum = sum.wrapping_add(j as u64);
                                }
                                criterion::black_box(sum);
                            }
                            
                            start.elapsed()
                        });
                        handles.push(handle);
                    }
                    
                    let times = futures::future::join_all(handles).await;
                    criterion::black_box(times)
                });
            },
        );
        
        // Simulated work stealing (shared task queue)
        group.bench_with_input(
            BenchmarkId::new("work_stealing", worker_count),
            &worker_count,
            |b, &workers| {
                b.to_async(&rt).iter(|| async {
                    let task_queue = Arc::new(Mutex::new((0..task_count).collect::<Vec<_>>()));
                    
                    let mut handles = Vec::new();
                    for _worker_id in 0..workers {
                        let queue_clone = task_queue.clone();
                        let handle = tokio::spawn(async move {
                            let start = tokio::time::Instant::now();
                            
                            loop {
                                let task = {
                                    let mut queue = queue_clone.lock().await;
                                    queue.pop()
                                };
                                
                                match task {
                                    Some(task_id) => {
                                        let work_amount = if task_id % 10 == 0 { 1000 } else { 100 };
                                        
                                        // Simulate CPU work
                                        let mut sum = 0u64;
                                        for j in 0..work_amount {
                                            sum = sum.wrapping_add(j as u64);
                                        }
                                        criterion::black_box(sum);
                                    }
                                    None => break,
                                }
                            }
                            
                            start.elapsed()
                        });
                        handles.push(handle);
                    }
                    
                    let times = futures::future::join_all(handles).await;
                    criterion::black_box(times)
                });
            },
        );
    }
    
    group.finish();
}

/// Mock concurrent test context.
struct ConcurrentTestContext {
    publisher: Publisher,
}

impl ConcurrentTestContext {
    fn clone(&self) -> Self {
        Self {
            publisher: Publisher,
        }
    }
    
    async fn create_consumer(&self, _consumer_id: &str) -> Result<Consumer> {
        Ok(Consumer)
    }
}

impl BenchmarkContext {
    async fn concurrent() -> ConcurrentTestContext {
        ConcurrentTestContext {
            publisher: Publisher,
        }
    }
}

criterion_group!(
    concurrent_benches,
    bench_multi_producer_single_consumer,
    bench_single_producer_multi_consumer,
    bench_multi_producer_multi_consumer,
    bench_lock_free_structures,
    bench_async_task_overhead,
    bench_contention_scenarios,
    bench_work_stealing
);

criterion_main!(concurrent_benches);