//! Utility functions and helpers for testing.

use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use std::collections::HashMap;

/// Utilities for timing and performance measurement.
pub mod timing {
    use super::*;

    /// High-precision timer for measuring operation latencies.
    pub struct LatencyTimer {
        start: Instant,
    }

    impl LatencyTimer {
        /// Start a new latency measurement.
        pub fn start() -> Self {
            Self {
                start: Instant::now(),
            }
        }

        /// Get elapsed time in microseconds.
        pub fn elapsed_micros(&self) -> u64 {
            self.start.elapsed().as_micros() as u64
        }

        /// Get elapsed time in nanoseconds.
        pub fn elapsed_nanos(&self) -> u64 {
            self.start.elapsed().as_nanos() as u64
        }

        /// Get elapsed duration.
        pub fn elapsed(&self) -> Duration {
            self.start.elapsed()
        }
    }

    /// Rate limiter for controlling operation frequency.
    pub struct RateLimiter {
        interval: Duration,
        last_execution: Arc<Mutex<Instant>>,
    }

    impl RateLimiter {
        /// Create a new rate limiter with the specified rate (operations per second).
        pub fn new(rate: f64) -> Self {
            let interval = Duration::from_nanos((1_000_000_000.0 / rate) as u64);
            Self {
                interval,
                last_execution: Arc::new(Mutex::new(Instant::now() - interval)),
            }
        }

        /// Wait until the next operation is allowed.
        pub async fn wait(&self) {
            let mut last = self.last_execution.lock().await;
            let now = Instant::now();
            let next_allowed = *last + self.interval;

            if now < next_allowed {
                tokio::time::sleep(next_allowed - now).await;
                *last = next_allowed;
            } else {
                *last = now;
            }
        }

        /// Check if an operation is allowed without waiting.
        pub async fn is_allowed(&self) -> bool {
            let last = self.last_execution.lock().await;
            let now = Instant::now();
            now >= *last + self.interval
        }
    }

    /// Throughput calculator for measuring operations per second.
    pub struct ThroughputCalculator {
        start_time: Instant,
        operation_count: AtomicU64,
    }

    impl ThroughputCalculator {
        /// Create a new throughput calculator.
        pub fn new() -> Self {
            Self {
                start_time: Instant::now(),
                operation_count: AtomicU64::new(0),
            }
        }

        /// Record an operation.
        pub fn record_operation(&self) {
            self.operation_count.fetch_add(1, Ordering::Relaxed);
        }

        /// Record multiple operations.
        pub fn record_operations(&self, count: u64) {
            self.operation_count.fetch_add(count, Ordering::Relaxed);
        }

        /// Get current throughput in operations per second.
        pub fn current_throughput(&self) -> f64 {
            let elapsed = self.start_time.elapsed().as_secs_f64();
            let operations = self.operation_count.load(Ordering::Relaxed) as f64;
            
            if elapsed > 0.0 {
                operations / elapsed
            } else {
                0.0
            }
        }

        /// Get total operations recorded.
        pub fn total_operations(&self) -> u64 {
            self.operation_count.load(Ordering::Relaxed)
        }

        /// Get elapsed time since creation.
        pub fn elapsed(&self) -> Duration {
            self.start_time.elapsed()
        }

        /// Reset the calculator.
        pub fn reset(&self) {
            self.operation_count.store(0, Ordering::Relaxed);
        }
    }

    impl Default for ThroughputCalculator {
        fn default() -> Self {
            Self::new()
        }
    }
}

/// Utilities for memory and resource monitoring.
pub mod monitoring {
    use super::*;

    /// Memory usage monitor.
    pub struct MemoryMonitor {
        initial_usage: u64,
        peak_usage: AtomicU64,
    }

    impl MemoryMonitor {
        /// Create a new memory monitor.
        pub fn new() -> Self {
            let initial = Self::current_memory_usage();
            Self {
                initial_usage: initial,
                peak_usage: AtomicU64::new(initial),
            }
        }

        /// Update peak memory usage if current usage is higher.
        pub fn update_peak(&self) {
            let current = Self::current_memory_usage();
            let mut peak = self.peak_usage.load(Ordering::Relaxed);
            
            while current > peak {
                match self.peak_usage.compare_exchange_weak(
                    peak, current, Ordering::Relaxed, Ordering::Relaxed
                ) {
                    Ok(_) => break,
                    Err(new_peak) => peak = new_peak,
                }
            }
        }

        /// Get current memory usage increase since monitor creation.
        pub fn memory_increase(&self) -> u64 {
            Self::current_memory_usage().saturating_sub(self.initial_usage)
        }

        /// Get peak memory usage since monitor creation.
        pub fn peak_memory_increase(&self) -> u64 {
            self.peak_usage.load(Ordering::Relaxed).saturating_sub(self.initial_usage)
        }

        /// Get current process memory usage in bytes.
        #[cfg(target_os = "linux")]
        fn current_memory_usage() -> u64 {
            use std::fs;
            
            if let Ok(statm) = fs::read_to_string("/proc/self/statm") {
                if let Some(pages) = statm.split_whitespace().next() {
                    if let Ok(pages) = pages.parse::<u64>() {
                        return pages * 4096; // Assume 4KB pages
                    }
                }
            }
            0
        }

        #[cfg(not(target_os = "linux"))]
        fn current_memory_usage() -> u64 {
            // Fallback implementation for non-Linux systems
            0
        }
    }

    impl Default for MemoryMonitor {
        fn default() -> Self {
            Self::new()
        }
    }

    /// CPU usage monitor (simplified).
    pub struct CpuMonitor {
        last_measurement: Arc<RwLock<Instant>>,
        usage_samples: Arc<RwLock<Vec<f64>>>,
    }

    impl CpuMonitor {
        /// Create a new CPU monitor.
        pub fn new() -> Self {
            Self {
                last_measurement: Arc::new(RwLock::new(Instant::now())),
                usage_samples: Arc::new(RwLock::new(Vec::new())),
            }
        }

        /// Record a CPU usage sample.
        pub async fn record_sample(&self, usage_percent: f64) {
            let mut samples = self.usage_samples.write().await;
            samples.push(usage_percent);
            
            // Keep only recent samples (last 100)
            if samples.len() > 100 {
                samples.remove(0);
            }
            
            *self.last_measurement.write().await = Instant::now();
        }

        /// Get average CPU usage from samples.
        pub async fn average_usage(&self) -> f64 {
            let samples = self.usage_samples.read().await;
            if samples.is_empty() {
                0.0
            } else {
                samples.iter().sum::<f64>() / samples.len() as f64
            }
        }

        /// Get peak CPU usage from samples.
        pub async fn peak_usage(&self) -> f64 {
            let samples = self.usage_samples.read().await;
            samples.iter().fold(0.0, |acc, &x| acc.max(x))
        }
    }

    impl Default for CpuMonitor {
        fn default() -> Self {
            Self::new()
        }
    }
}

/// Utilities for concurrent testing.
pub mod concurrency {
    use super::*;
    use std::sync::Barrier;

    /// Countdown latch for coordinating multiple tasks.
    pub struct CountdownLatch {
        count: AtomicU64,
    }

    impl CountdownLatch {
        /// Create a new countdown latch with the specified count.
        pub fn new(count: u64) -> Self {
            Self {
                count: AtomicU64::new(count),
            }
        }

        /// Decrement the count and return the new value.
        pub fn count_down(&self) -> u64 {
            self.count.fetch_sub(1, Ordering::SeqCst)
        }

        /// Wait until the count reaches zero.
        pub async fn wait(&self) {
            while self.count.load(Ordering::SeqCst) > 0 {
                tokio::task::yield_now().await;
            }
        }

        /// Get the current count.
        pub fn count(&self) -> u64 {
            self.count.load(Ordering::SeqCst)
        }
    }

    /// Shared counter for coordinating between tasks.
    pub struct SharedCounter {
        value: AtomicU64,
    }

    impl SharedCounter {
        /// Create a new shared counter.
        pub fn new(initial_value: u64) -> Self {
            Self {
                value: AtomicU64::new(initial_value),
            }
        }

        /// Increment and return the new value.
        pub fn increment(&self) -> u64 {
            self.value.fetch_add(1, Ordering::SeqCst) + 1
        }

        /// Decrement and return the new value.
        pub fn decrement(&self) -> u64 {
            self.value.fetch_sub(1, Ordering::SeqCst) - 1
        }

        /// Add a value and return the new total.
        pub fn add(&self, value: u64) -> u64 {
            self.value.fetch_add(value, Ordering::SeqCst) + value
        }

        /// Get the current value.
        pub fn get(&self) -> u64 {
            self.value.load(Ordering::SeqCst)
        }

        /// Set a new value and return the old value.
        pub fn set(&self, value: u64) -> u64 {
            self.value.swap(value, Ordering::SeqCst)
        }
    }

    impl Default for SharedCounter {
        fn default() -> Self {
            Self::new(0)
        }
    }

    /// Task coordination utilities.
    pub struct TaskCoordinator {
        task_count: AtomicU64,
        completed_count: AtomicU64,
        barrier: Option<Arc<Barrier>>,
    }

    impl TaskCoordinator {
        /// Create a new task coordinator.
        pub fn new() -> Self {
            Self {
                task_count: AtomicU64::new(0),
                completed_count: AtomicU64::new(0),
                barrier: None,
            }
        }

        /// Create a coordinator with a barrier for the specified number of tasks.
        pub fn with_barrier(task_count: usize) -> Self {
            Self {
                task_count: AtomicU64::new(task_count as u64),
                completed_count: AtomicU64::new(0),
                barrier: Some(Arc::new(Barrier::new(task_count))),
            }
        }

        /// Register a task.
        pub fn register_task(&self) {
            self.task_count.fetch_add(1, Ordering::SeqCst);
        }

        /// Mark a task as completed.
        pub fn complete_task(&self) {
            self.completed_count.fetch_add(1, Ordering::SeqCst);
        }

        /// Wait at the barrier (if configured).
        pub fn wait_at_barrier(&self) {
            if let Some(ref barrier) = self.barrier {
                barrier.wait();
            }
        }

        /// Check if all tasks are completed.
        pub fn all_completed(&self) -> bool {
            let total = self.task_count.load(Ordering::SeqCst);
            let completed = self.completed_count.load(Ordering::SeqCst);
            total > 0 && completed >= total
        }

        /// Get completion progress (0.0 to 1.0).
        pub fn progress(&self) -> f64 {
            let total = self.task_count.load(Ordering::SeqCst);
            let completed = self.completed_count.load(Ordering::SeqCst);
            
            if total == 0 {
                0.0
            } else {
                completed as f64 / total as f64
            }
        }
    }

    impl Default for TaskCoordinator {
        fn default() -> Self {
            Self::new()
        }
    }
}

/// Utilities for working with test data.
pub mod data {
    use super::*;
    use kaelix_core::{Message, MessageId};

    /// Message collector for gathering test results.
    pub struct MessageCollector {
        messages: Arc<RwLock<Vec<Message>>>,
        metadata: Arc<RwLock<HashMap<MessageId, MessageMetadata>>>,
    }

    /// Metadata about a collected message.
    #[derive(Debug, Clone)]
    pub struct MessageMetadata {
        /// When the message was collected
        pub collected_at: Instant,
        /// Source of the message (e.g., "publisher-1", "consumer-2")
        pub source: String,
        /// Additional metadata
        pub extra: HashMap<String, String>,
    }

    impl MessageCollector {
        /// Create a new message collector.
        pub fn new() -> Self {
            Self {
                messages: Arc::new(RwLock::new(Vec::new())),
                metadata: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        /// Collect a message with metadata.
        pub async fn collect(&self, message: Message, source: String) {
            let metadata = MessageMetadata {
                collected_at: Instant::now(),
                source,
                extra: HashMap::new(),
            };

            let message_id = message.id;
            
            {
                let mut messages = self.messages.write().await;
                messages.push(message);
            }
            
            {
                let mut meta = self.metadata.write().await;
                meta.insert(message_id, metadata);
            }
        }

        /// Get all collected messages.
        pub async fn messages(&self) -> Vec<Message> {
            self.messages.read().await.clone()
        }

        /// Get metadata for a message.
        pub async fn metadata(&self, message_id: &MessageId) -> Option<MessageMetadata> {
            self.metadata.read().await.get(message_id).cloned()
        }

        /// Get count of collected messages.
        pub async fn count(&self) -> usize {
            self.messages.read().await.len()
        }

        /// Clear all collected data.
        pub async fn clear(&self) {
            self.messages.write().await.clear();
            self.metadata.write().await.clear();
        }

        /// Get messages by source.
        pub async fn messages_by_source(&self, source: &str) -> Vec<Message> {
            let messages = self.messages.read().await;
            let metadata = self.metadata.read().await;
            
            let mut result = Vec::new();
            for message in messages.iter() {
                if let Some(meta) = metadata.get(&message.id) {
                    if meta.source == source {
                        result.push(message.clone());
                    }
                }
            }
            result
        }
    }

    impl Default for MessageCollector {
        fn default() -> Self {
            Self::new()
        }
    }
}

/// Utilities for error injection and testing.
pub mod errors {
    use super::*;
    use rand::Rng;

    /// Error injector for testing error handling.
    pub struct ErrorInjector {
        failure_rate: f64,
        enabled: AtomicU64, // Using as bool (0/1)
    }

    impl ErrorInjector {
        /// Create a new error injector with the specified failure rate (0.0 to 1.0).
        pub fn new(failure_rate: f64) -> Self {
            Self {
                failure_rate: failure_rate.clamp(0.0, 1.0),
                enabled: AtomicU64::new(1),
            }
        }

        /// Check if an error should be injected.
        pub fn should_inject_error(&self) -> bool {
            if self.enabled.load(Ordering::Relaxed) == 0 {
                return false;
            }

            let mut rng = rand::thread_rng();
            rng.gen::<f64>() < self.failure_rate
        }

        /// Enable error injection.
        pub fn enable(&self) {
            self.enabled.store(1, Ordering::Relaxed);
        }

        /// Disable error injection.
        pub fn disable(&self) {
            self.enabled.store(0, Ordering::Relaxed);
        }

        /// Check if error injection is enabled.
        pub fn is_enabled(&self) -> bool {
            self.enabled.load(Ordering::Relaxed) == 1
        }

        /// Set failure rate.
        pub fn set_failure_rate(&mut self, rate: f64) {
            self.failure_rate = rate.clamp(0.0, 1.0);
        }
    }
}