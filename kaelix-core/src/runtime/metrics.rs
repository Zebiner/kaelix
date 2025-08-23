//! Runtime Performance Metrics
//!
//! Comprehensive performance monitoring and latency tracking for the async runtime.
//! Provides real-time metrics collection with minimal overhead.

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

/// High-resolution latency histogram for P99/P95/P50 tracking
pub struct LatencyHistogram {
    /// Histogram buckets (microseconds) - logarithmic scale
    buckets: Vec<AtomicU64>,
    /// Bucket boundaries in microseconds
    boundaries: Vec<u64>,
    /// Total sample count for percentile calculations
    total_samples: AtomicU64,
    /// Sum of all latencies for mean calculation
    sum_latencies: AtomicU64,
    /// Minimum observed latency
    min_latency: AtomicU64,
    /// Maximum observed latency
    max_latency: AtomicU64,
}

impl LatencyHistogram {
    /// Create a new latency histogram with logarithmic bucketing
    pub fn new() -> Self {
        // Logarithmic buckets: 1μs, 2μs, 5μs, 10μs, 20μs, 50μs, 100μs, 200μs, 500μs, 1ms, 2ms, 5ms, 10ms, 20ms, 50ms, 100ms, 200ms, 500ms, 1s, 2s, 5s, 10s
        let boundaries = vec![
            1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000, 10_000, 20_000, 50_000,
            100_000, 200_000, 500_000, 1_000_000, 2_000_000, 5_000_000, 10_000_000,
        ];

        let buckets = (0..boundaries.len() + 1).map(|_| AtomicU64::new(0)).collect();

        Self {
            buckets,
            boundaries,
            total_samples: AtomicU64::new(0),
            sum_latencies: AtomicU64::new(0),
            min_latency: AtomicU64::new(u64::MAX),
            max_latency: AtomicU64::new(0),
        }
    }

    /// Record a latency sample
    pub fn record(&self, latency_us: u64) {
        // Find the appropriate bucket
        let bucket_idx = self.boundaries.binary_search(&latency_us).unwrap_or_else(|i| i);

        self.buckets[bucket_idx].fetch_add(1, Ordering::Relaxed);
        self.total_samples.fetch_add(1, Ordering::Relaxed);
        self.sum_latencies.fetch_add(latency_us, Ordering::Relaxed);

        // Update min/max
        let current_min = self.min_latency.load(Ordering::Relaxed);
        if latency_us < current_min {
            self.min_latency.store(latency_us, Ordering::Relaxed);
        }

        let current_max = self.max_latency.load(Ordering::Relaxed);
        if latency_us > current_max {
            self.max_latency.store(latency_us, Ordering::Relaxed);
        }
    }

    /// Calculate percentile (50.0 for P50, 95.0 for P95, 99.0 for P99)
    pub fn percentile(&self, percentile: f64) -> u64 {
        let total = self.total_samples.load(Ordering::Relaxed);
        if total == 0 {
            return 0;
        }

        let target_count = (total as f64 * percentile / 100.0) as u64;
        let mut running_count = 0;

        for (i, bucket) in self.buckets.iter().enumerate() {
            running_count += bucket.load(Ordering::Relaxed);
            if running_count >= target_count {
                return if i == 0 {
                    self.boundaries[0] / 2 // Estimate for first bucket
                } else if i >= self.boundaries.len() {
                    self.boundaries[self.boundaries.len() - 1] * 2 // Estimate for overflow bucket
                } else {
                    self.boundaries[i - 1]
                };
            }
        }

        self.max_latency.load(Ordering::Relaxed)
    }

    /// Get mean latency
    pub fn mean(&self) -> u64 {
        let total = self.total_samples.load(Ordering::Relaxed);
        if total == 0 {
            0
        } else {
            self.sum_latencies.load(Ordering::Relaxed) / total
        }
    }

    /// Get minimum latency
    pub fn min(&self) -> u64 {
        let min = self.min_latency.load(Ordering::Relaxed);
        if min == u64::MAX {
            0
        } else {
            min
        }
    }

    /// Get maximum latency
    pub fn max(&self) -> u64 {
        self.max_latency.load(Ordering::Relaxed)
    }

    /// Get total sample count
    pub fn count(&self) -> u64 {
        self.total_samples.load(Ordering::Relaxed)
    }

    /// Reset all metrics
    pub fn reset(&self) {
        for bucket in &self.buckets {
            bucket.store(0, Ordering::Relaxed);
        }
        self.total_samples.store(0, Ordering::Relaxed);
        self.sum_latencies.store(0, Ordering::Relaxed);
        self.min_latency.store(u64::MAX, Ordering::Relaxed);
        self.max_latency.store(0, Ordering::Relaxed);
    }
}

/// Runtime metrics collector
pub struct RuntimeMetrics {
    /// Task scheduling latency
    pub task_latency: LatencyHistogram,
    /// I/O operation latency
    pub io_latency: LatencyHistogram,
    /// CPU utilization tracking
    pub cpu_utilization: AtomicU64, // Percentage * 100 for precision
    /// Memory usage in bytes
    pub memory_usage: AtomicU64,
    /// Active task count
    pub active_tasks: AtomicUsize,
    /// Completed task count
    pub completed_tasks: AtomicU64,
    /// Failed task count
    pub failed_tasks: AtomicU64,
    /// Thread pool statistics
    pub thread_pool_stats: Arc<ThreadPoolStats>,
    /// GC (if applicable) statistics
    pub gc_stats: Arc<RwLock<GcStats>>,
    /// Creation timestamp
    created_at: Instant,
}

impl RuntimeMetrics {
    pub fn new() -> Self {
        Self {
            task_latency: LatencyHistogram::new(),
            io_latency: LatencyHistogram::new(),
            cpu_utilization: AtomicU64::new(0),
            memory_usage: AtomicU64::new(0),
            active_tasks: AtomicUsize::new(0),
            completed_tasks: AtomicU64::new(0),
            failed_tasks: AtomicU64::new(0),
            thread_pool_stats: Arc::new(ThreadPoolStats::new()),
            gc_stats: Arc::new(RwLock::new(GcStats::new())),
            created_at: Instant::now(),
        }
    }

    /// Record task completion
    pub fn record_task_completion(&self, duration: Duration) {
        let duration_us = duration.as_micros() as u64;
        self.task_latency.record(duration_us);
        self.active_tasks.fetch_sub(1, Ordering::Relaxed);
        self.completed_tasks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record task start
    pub fn record_task_start(&self) {
        self.active_tasks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record task failure
    pub fn record_task_failure(&self, duration: Duration) {
        let duration_us = duration.as_micros() as u64;
        self.task_latency.record(duration_us);
        self.active_tasks.fetch_sub(1, Ordering::Relaxed);
        self.failed_tasks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record I/O operation
    pub fn record_io_operation(&self, duration: Duration) {
        let duration_us = duration.as_micros() as u64;
        self.io_latency.record(duration_us);
    }

    /// Update CPU utilization (percentage * 100)
    pub fn update_cpu_utilization(&self, utilization_percent_x100: u64) {
        self.cpu_utilization.store(utilization_percent_x100, Ordering::Relaxed);
    }

    /// Update memory usage in bytes
    pub fn update_memory_usage(&self, bytes: u64) {
        self.memory_usage.store(bytes, Ordering::Relaxed);
    }

    /// Get comprehensive metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        let gc_stats = self.gc_stats.read().clone();
        let thread_pool_stats = self.thread_pool_stats.snapshot();

        MetricsSnapshot {
            timestamp: Utc::now(),
            uptime_seconds: self.created_at.elapsed().as_secs(),
            task_metrics: TaskMetrics {
                active_count: self.active_tasks.load(Ordering::Relaxed),
                completed_count: self.completed_tasks.load(Ordering::Relaxed),
                failed_count: self.failed_tasks.load(Ordering::Relaxed),
                p50_latency_us: self.task_latency.percentile(50.0),
                p95_latency_us: self.task_latency.percentile(95.0),
                p99_latency_us: self.task_latency.percentile(99.0),
                mean_latency_us: self.task_latency.mean(),
                min_latency_us: self.task_latency.min(),
                max_latency_us: self.task_latency.max(),
            },
            io_metrics: IoMetrics {
                p50_latency_us: self.io_latency.percentile(50.0),
                p95_latency_us: self.io_latency.percentile(95.0),
                p99_latency_us: self.io_latency.percentile(99.0),
                mean_latency_us: self.io_latency.mean(),
                min_latency_us: self.io_latency.min(),
                max_latency_us: self.io_latency.max(),
                operation_count: self.io_latency.count(),
            },
            system_metrics: SystemMetrics {
                cpu_utilization_percent: (self.cpu_utilization.load(Ordering::Relaxed) as f64)
                    / 100.0,
                memory_usage_bytes: self.memory_usage.load(Ordering::Relaxed),
            },
            thread_pool_stats,
            gc_stats,
        }
    }
}

/// Thread pool statistics
pub struct ThreadPoolStats {
    /// Number of worker threads
    pub worker_count: AtomicUsize,
    /// Number of queued tasks
    pub queued_tasks: AtomicUsize,
    /// Thread utilization
    pub thread_utilization: AtomicU64, // Percentage * 100
}

impl ThreadPoolStats {
    pub fn new() -> Self {
        Self {
            worker_count: AtomicUsize::new(0),
            queued_tasks: AtomicUsize::new(0),
            thread_utilization: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> ThreadPoolStatsSnapshot {
        ThreadPoolStatsSnapshot {
            worker_count: self.worker_count.load(Ordering::Relaxed),
            queued_tasks: self.queued_tasks.load(Ordering::Relaxed),
            thread_utilization_percent: (self.thread_utilization.load(Ordering::Relaxed) as f64)
                / 100.0,
        }
    }
}

/// Garbage collection statistics (for future use)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcStats {
    pub total_collections: u64,
    pub total_gc_time_ms: u64,
    pub average_gc_time_ms: f64,
    pub memory_freed_bytes: u64,
    pub memory_allocated_bytes: u64,
}

impl GcStats {
    pub fn new() -> Self {
        Self {
            total_collections: 0,
            total_gc_time_ms: 0,
            average_gc_time_ms: 0.0,
            memory_freed_bytes: 0,
            memory_allocated_bytes: 0,
        }
    }
}

/// Comprehensive metrics snapshot for telemetry export
#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub uptime_seconds: u64,
    pub task_metrics: TaskMetrics,
    pub io_metrics: IoMetrics,
    pub system_metrics: SystemMetrics,
    pub thread_pool_stats: ThreadPoolStatsSnapshot,
    pub gc_stats: GcStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskMetrics {
    pub active_count: usize,
    pub completed_count: u64,
    pub failed_count: u64,
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub p99_latency_us: u64,
    pub mean_latency_us: u64,
    pub min_latency_us: u64,
    pub max_latency_us: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IoMetrics {
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub p99_latency_us: u64,
    pub mean_latency_us: u64,
    pub min_latency_us: u64,
    pub max_latency_us: u64,
    pub operation_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_utilization_percent: f64,
    pub memory_usage_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadPoolStatsSnapshot {
    pub worker_count: usize,
    pub queued_tasks: usize,
    pub thread_utilization_percent: f64,
}

/// Metrics collection frequency configuration
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// How often to collect system metrics
    pub collection_interval: Duration,
    /// Whether to enable high-resolution latency tracking
    pub enable_high_resolution: bool,
    /// Maximum number of samples to keep in memory
    pub max_samples: usize,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            collection_interval: Duration::from_secs(1),
            enable_high_resolution: true,
            max_samples: 10_000,
        }
    }
}

/// Global metrics registry for the runtime
pub struct MetricsRegistry {
    /// Primary runtime metrics
    pub runtime: Arc<RuntimeMetrics>,
    /// Configuration
    config: MetricsConfig,
    /// Collection task handle
    _collection_handle: tokio::task::JoinHandle<()>,
}

impl MetricsRegistry {
    /// Create a new metrics registry with periodic collection
    pub fn new(config: MetricsConfig) -> Self {
        let runtime = Arc::new(RuntimeMetrics::new());
        let runtime_clone = runtime.clone();
        let interval = config.collection_interval;

        let collection_handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            loop {
                interval_timer.tick().await;
                Self::collect_system_metrics(&runtime_clone).await;
            }
        });

        Self { runtime, config, _collection_handle: collection_handle }
    }

    /// Collect system-wide metrics (CPU, memory, etc.)
    async fn collect_system_metrics(metrics: &RuntimeMetrics) {
        // In a real implementation, this would use platform-specific APIs
        // For now, we'll simulate with placeholder values

        // Simulate CPU utilization collection
        let cpu_util = Self::get_cpu_utilization().await;
        metrics.update_cpu_utilization((cpu_util * 100.0) as u64);

        // Simulate memory usage collection
        let memory_usage = Self::get_memory_usage().await;
        metrics.update_memory_usage(memory_usage);

        // Update thread pool stats
        let thread_count = tokio::runtime::Handle::current().metrics().num_workers();
        metrics.thread_pool_stats.worker_count.store(thread_count, Ordering::Relaxed);
    }

    /// Get current CPU utilization (placeholder implementation)
    async fn get_cpu_utilization() -> f64 {
        // This would use platform-specific APIs like /proc/stat on Linux
        // or GetSystemTimes on Windows
        0.0 // Placeholder
    }

    /// Get current memory usage (placeholder implementation)
    async fn get_memory_usage() -> u64 {
        // This would use platform-specific APIs for actual memory usage
        0 // Placeholder
    }

    /// Get a snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        self.runtime.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_histogram() {
        let histogram = LatencyHistogram::new();

        // Record some sample latencies
        histogram.record(100); // 100 microseconds
        histogram.record(500); // 500 microseconds
        histogram.record(1000); // 1 millisecond
        histogram.record(5000); // 5 milliseconds

        assert_eq!(histogram.count(), 4);
        assert!(histogram.mean() > 0);
        assert!(histogram.percentile(50.0) > 0);
        assert!(histogram.percentile(95.0) > 0);
        assert!(histogram.percentile(99.0) > 0);
    }

    #[test]
    fn test_runtime_metrics() {
        let metrics = RuntimeMetrics::new();

        metrics.record_task_start();
        metrics.record_task_completion(Duration::from_millis(10));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.task_metrics.completed_count, 1);
        assert_eq!(snapshot.task_metrics.active_count, 0);
    }

    #[tokio::test]
    async fn test_metrics_registry() {
        let config = MetricsConfig::default();
        let registry = MetricsRegistry::new(config);

        // Record some metrics
        registry.runtime.record_task_start();
        registry.runtime.record_task_completion(Duration::from_millis(5));

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.task_metrics.completed_count, 1);
    }
}
