//! Runtime Performance Metrics
//!
//! Comprehensive performance monitoring and latency tracking for the async runtime.
//! Provides real-time metrics collection with minimal overhead.

use crate::runtime::performance::*;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use parking_lot::RwLock;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

/// High-resolution latency histogram for P99/P95/P50 tracking
pub struct LatencyHistogram {
    /// Histogram buckets (in nanoseconds)
    buckets: Vec<AtomicU64>,
    
    /// Bucket boundaries (in nanoseconds)
    bucket_boundaries: Vec<u64>,
    
    /// Total samples recorded
    total_samples: AtomicU64,
    
    /// Sum of all latencies (for average calculation)
    total_latency_ns: AtomicU64,
    
    /// Target P99 latency for violation tracking
    p99_target: Duration,
    
    /// Number of P99 violations
    violation_count: AtomicU64,
    
    /// Minimum recorded latency
    min_latency: AtomicU64,
    
    /// Maximum recorded latency
    max_latency: AtomicU64,
}

impl LatencyHistogram {
    /// Create a new latency histogram with optimized buckets for microsecond precision
    pub fn new(p99_target: Duration) -> Self {
        // Create buckets optimized for microsecond-scale latencies
        // Buckets: 0-1μs, 1-2μs, 2-5μs, 5-10μs, 10-20μs, 20-50μs, 50-100μs, 100μs+
        let bucket_boundaries = vec![
            1_000,     // 1μs
            2_000,     // 2μs
            5_000,     // 5μs
            10_000,    // 10μs
            20_000,    // 20μs
            50_000,    // 50μs
            100_000,   // 100μs
            500_000,   // 500μs
            1_000_000, // 1ms
            10_000_000, // 10ms
            u64::MAX,  // overflow bucket
        ];
        
        let buckets = bucket_boundaries
            .iter()
            .map(|_| AtomicU64::new(0))
            .collect();
        
        Self {
            buckets,
            bucket_boundaries,
            total_samples: AtomicU64::new(0),
            total_latency_ns: AtomicU64::new(0),
            p99_target,
            violation_count: AtomicU64::new(0),
            min_latency: AtomicU64::new(u64::MAX),
            max_latency: AtomicU64::new(0),
        }
    }
    
    /// Record a latency measurement
    pub fn record(&self, latency: Duration) {
        let latency_ns = latency.as_nanos() as u64;
        
        // Update min/max
        self.min_latency.fetch_min(latency_ns, Ordering::Relaxed);
        self.max_latency.fetch_max(latency_ns, Ordering::Relaxed);
        
        // Update totals
        self.total_samples.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);
        
        // Find the appropriate bucket
        for (i, &boundary) in self.bucket_boundaries.iter().enumerate() {
            if latency_ns <= boundary {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
        
        // Check for P99 target violation
        if latency > self.p99_target {
            self.violation_count.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Calculate P99 latency
    pub fn p99(&self) -> Duration {
        self.percentile(99.0)
    }
    
    /// Calculate P95 latency
    pub fn p95(&self) -> Duration {
        self.percentile(95.0)
    }
    
    /// Calculate P90 latency
    pub fn p90(&self) -> Duration {
        self.percentile(90.0)
    }
    
    /// Calculate P50 (median) latency
    pub fn p50(&self) -> Duration {
        self.percentile(50.0)
    }
    
    /// Calculate arbitrary percentile
    fn percentile(&self, percentile: f64) -> Duration {
        let total = self.total_samples.load(Ordering::Relaxed);
        if total == 0 {
            return Duration::ZERO;
        }
        
        let target_count = ((total as f64) * (percentile / 100.0)) as u64;
        let mut count = 0;
        
        for (i, bucket) in self.buckets.iter().enumerate() {
            count += bucket.load(Ordering::Relaxed);
            if count >= target_count {
                // Return the boundary value for this bucket
                let boundary_ns = if i == 0 {
                    self.bucket_boundaries[i] / 2 // Midpoint of first bucket
                } else {
                    (self.bucket_boundaries[i - 1] + self.bucket_boundaries[i]) / 2 // Midpoint
                };
                return Duration::from_nanos(boundary_ns);
            }
        }
        
        // Fallback to max if we somehow didn't find it
        Duration::from_nanos(self.max_latency.load(Ordering::Relaxed))
    }
    
    /// Get average latency
    pub fn average(&self) -> Duration {
        let total = self.total_samples.load(Ordering::Relaxed);
        if total == 0 {
            Duration::ZERO
        } else {
            let total_ns = self.total_latency_ns.load(Ordering::Relaxed);
            Duration::from_nanos(total_ns / total)
        }
    }
    
    /// Get minimum latency
    pub fn min(&self) -> Duration {
        let min_ns = self.min_latency.load(Ordering::Relaxed);
        if min_ns == u64::MAX {
            Duration::ZERO
        } else {
            Duration::from_nanos(min_ns)
        }
    }
    
    /// Get maximum latency
    pub fn max(&self) -> Duration {
        Duration::from_nanos(self.max_latency.load(Ordering::Relaxed))
    }
    
    /// Get total number of samples
    pub fn sample_count(&self) -> u64 {
        self.total_samples.load(Ordering::Relaxed)
    }
    
    /// Get number of P99 target violations
    pub fn violations(&self) -> u64 {
        self.violation_count.load(Ordering::Relaxed)
    }
    
    /// Get violation rate (0.0 to 1.0)
    pub fn violation_rate(&self) -> f64 {
        let total = self.total_samples.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            let violations = self.violation_count.load(Ordering::Relaxed);
            violations as f64 / total as f64
        }
    }
    
    /// Reset all statistics
    pub fn reset(&self) {
        for bucket in &self.buckets {
            bucket.store(0, Ordering::Relaxed);
        }
        self.total_samples.store(0, Ordering::Relaxed);
        self.total_latency_ns.store(0, Ordering::Relaxed);
        self.violation_count.store(0, Ordering::Relaxed);
        self.min_latency.store(u64::MAX, Ordering::Relaxed);
        self.max_latency.store(0, Ordering::Relaxed);
    }
    
    /// Get histogram distribution
    pub fn distribution(&self) -> Vec<(Duration, u64)> {
        let mut result = Vec::new();
        
        for (i, bucket) in self.buckets.iter().enumerate() {
            let count = bucket.load(Ordering::Relaxed);
            if count > 0 {
                let boundary_ns = self.bucket_boundaries[i];
                result.push((Duration::from_nanos(boundary_ns), count));
            }
        }
        
        result
    }
}

/// Throughput measurement with time-based windows
pub struct ThroughputMeter {
    /// Samples in 1-second windows
    windows: Vec<AtomicU64>,
    
    /// Current window index
    current_window: AtomicUsize,
    
    /// Last window update time
    last_update: Arc<RwLock<Instant>>,
    
    /// Window size in seconds
    window_size: Duration,
    
    /// Number of windows to maintain
    window_count: usize,
}

impl ThroughputMeter {
    /// Create a new throughput meter
    pub fn new(window_size: Duration, window_count: usize) -> Self {
        let windows = (0..window_count)
            .map(|_| AtomicU64::new(0))
            .collect();
        
        Self {
            windows,
            current_window: AtomicUsize::new(0),
            last_update: Arc::new(RwLock::new(Instant::now())),
            window_size,
            window_count,
        }
    }
    
    /// Record an event (increment counter)
    pub fn record(&self, count: u64) {
        self.maybe_advance_window();
        
        let current = self.current_window.load(Ordering::Relaxed);
        self.windows[current].fetch_add(count, Ordering::Relaxed);
    }
    
    /// Get current throughput (events per second)
    pub fn current_rate(&self) -> f64 {
        self.maybe_advance_window();
        
        let current = self.current_window.load(Ordering::Relaxed);
        let count = self.windows[current].load(Ordering::Relaxed);
        
        count as f64 / self.window_size.as_secs_f64()
    }
    
    /// Get average throughput over all windows
    pub fn average_rate(&self) -> f64 {
        self.maybe_advance_window();
        
        let total: u64 = self.windows
            .iter()
            .map(|w| w.load(Ordering::Relaxed))
            .sum();
        
        let total_time = self.window_size.as_secs_f64() * self.window_count as f64;
        total as f64 / total_time
    }
    
    /// Get peak throughput across all windows
    pub fn peak_rate(&self) -> f64 {
        self.maybe_advance_window();
        
        let max_count = self.windows
            .iter()
            .map(|w| w.load(Ordering::Relaxed))
            .max()
            .unwrap_or(0);
        
        max_count as f64 / self.window_size.as_secs_f64()
    }
    
    /// Advance to next window if enough time has passed
    fn maybe_advance_window(&self) {
        let now = Instant::now();
        let mut last_update = self.last_update.write();
        
        if now.duration_since(*last_update) >= self.window_size {
            // Advance to next window
            let current = self.current_window.load(Ordering::Relaxed);
            let next = (current + 1) % self.window_count;
            
            // Clear the new window
            self.windows[next].store(0, Ordering::Relaxed);
            
            // Update current window
            self.current_window.store(next, Ordering::Relaxed);
            *last_update = now;
        }
    }
}

// Custom serialization for Duration
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs_f64().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = f64::deserialize(deserializer)?;
        Ok(Duration::from_secs_f64(secs))
    }
}

/// Worker utilization report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtilizationReport {
    /// Per-worker utilization statistics
    pub workers: Vec<WorkerUtilization>,
    
    /// Overall system utilization
    pub system_utilization: f64,
    
    /// Load balance score (0.0 = perfect balance, 1.0 = completely imbalanced)
    pub load_balance_score: f64,
    
    /// Timestamp of this report
    pub timestamp: DateTime<Utc>,
}

/// Individual worker utilization statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerUtilization {
    /// Worker ID
    pub worker_id: usize,
    
    /// CPU utilization (0.0 to 1.0)
    pub cpu_utilization: f64,
    
    /// Task execution rate (tasks per second)
    pub task_rate: f64,
    
    /// Average task execution time
    #[serde(with = "duration_serde")]
    pub avg_task_time: Duration,
    
    /// Work steal success rate
    pub steal_success_rate: f64,
    
    /// Current queue depth
    pub queue_depth: usize,
}

/// Comprehensive runtime metrics collection
pub struct RuntimeMetrics {
    /// Task execution latency histogram
    task_latency: LatencyHistogram,
    
    /// Task scheduling overhead histogram
    scheduling_overhead: LatencyHistogram,
    
    /// End-to-end message latency histogram
    message_latency: LatencyHistogram,
    
    /// Throughput meter for tasks per second
    task_throughput: ThroughputMeter,
    
    /// Throughput meter for messages per second
    message_throughput: ThroughputMeter,
    
    /// Per-worker utilization tracking
    worker_utilization: Vec<AtomicU64>,
    
    /// Per-worker task counts
    worker_task_counts: Vec<AtomicU64>,
    
    /// Per-worker queue depths
    worker_queue_depths: Vec<AtomicUsize>,
    
    /// Total tasks executed
    total_tasks_executed: AtomicU64,
    
    /// Total messages processed
    total_messages_processed: AtomicU64,
    
    /// Number of work steal attempts
    steal_attempts: AtomicU64,
    
    /// Number of successful work steals
    successful_steals: AtomicU64,
    
    /// NUMA locality hits (tasks executed on preferred NUMA node)
    numa_locality_hits: AtomicU64,
    
    /// NUMA locality misses
    numa_locality_misses: AtomicU64,
    
    /// Target latency for violation tracking
    target_latency: Duration,
    
    /// Target throughput for performance monitoring
    target_throughput: u64,
    
    /// Number of workers being tracked
    worker_count: usize,
}

impl RuntimeMetrics {
    /// Create new runtime metrics
    pub fn new(target_latency: Duration, target_throughput: u64, worker_count: usize) -> Self {
        Self {
            task_latency: LatencyHistogram::new(target_latency),
            scheduling_overhead: LatencyHistogram::new(Duration::from_micros(1)),
            message_latency: LatencyHistogram::new(target_latency),
            task_throughput: ThroughputMeter::new(Duration::from_secs(1), 60), // 1-minute window
            message_throughput: ThroughputMeter::new(Duration::from_secs(1), 60),
            worker_utilization: (0..worker_count)
                .map(|_| AtomicU64::new(0))
                .collect(),
            worker_task_counts: (0..worker_count)
                .map(|_| AtomicU64::new(0))
                .collect(),
            worker_queue_depths: (0..worker_count)
                .map(|_| AtomicUsize::new(0))
                .collect(),
            total_tasks_executed: AtomicU64::new(0),
            total_messages_processed: AtomicU64::new(0),
            steal_attempts: AtomicU64::new(0),
            successful_steals: AtomicU64::new(0),
            numa_locality_hits: AtomicU64::new(0),
            numa_locality_misses: AtomicU64::new(0),
            target_latency,
            target_throughput,
            worker_count,
        }
    }
    
    /// Record task execution latency
    pub fn record_task_latency(&self, latency: Duration) {
        self.task_latency.record(latency);
        self.task_throughput.record(1);
    }
    
    /// Record task scheduling overhead
    pub fn record_scheduling_overhead(&self, overhead: Duration) {
        self.scheduling_overhead.record(overhead);
    }
    
    /// Record end-to-end message latency
    pub fn record_message_latency(&self, latency: Duration) {
        self.message_latency.record(latency);
        self.message_throughput.record(1);
        self.total_messages_processed.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record task completion
    pub fn record_task_completion(&self, _task_age: Duration) {
        self.total_tasks_executed.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record work steal attempt
    pub fn record_steal_attempt(&self, successful: bool) {
        self.steal_attempts.fetch_add(1, Ordering::Relaxed);
        if successful {
            self.successful_steals.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Record NUMA locality
    pub fn record_numa_locality(&self, local: bool) {
        if local {
            self.numa_locality_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.numa_locality_misses.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Update worker utilization
    pub fn update_worker_utilization(&self, worker_id: usize, utilization_percent: u64) {
        if worker_id < self.worker_utilization.len() {
            self.worker_utilization[worker_id].store(utilization_percent, Ordering::Relaxed);
        }
    }
    
    /// Update worker queue depth
    pub fn update_worker_queue_depth(&self, worker_id: usize, depth: usize) {
        if worker_id < self.worker_queue_depths.len() {
            self.worker_queue_depths[worker_id].store(depth, Ordering::Relaxed);
        }
    }
    
    /// Get P99 task latency
    pub fn p99_latency(&self) -> Duration {
        self.task_latency.p99()
    }
    
    /// Get P95 task latency
    pub fn p95_latency(&self) -> Duration {
        self.task_latency.p95()
    }
    
    /// Get P50 task latency
    pub fn p50_latency(&self) -> Duration {
        self.task_latency.p50()
    }
    
    /// Get average task latency
    pub fn average_latency(&self) -> Duration {
        self.task_latency.average()
    }
    
    /// Get current task throughput (tasks per second)
    pub fn current_task_throughput(&self) -> f64 {
        self.task_throughput.current_rate()
    }
    
    /// Get current message throughput (messages per second)
    pub fn current_message_throughput(&self) -> f64 {
        self.message_throughput.current_rate()
    }
    
    /// Get average task throughput
    pub fn average_task_throughput(&self) -> f64 {
        self.task_throughput.average_rate()
    }
    
    /// Get average message throughput
    pub fn average_message_throughput(&self) -> f64 {
        self.message_throughput.average_rate()
    }
    
    /// Get peak task throughput
    pub fn peak_task_throughput(&self) -> f64 {
        self.task_throughput.peak_rate()
    }
    
    /// Get peak message throughput
    pub fn peak_message_throughput(&self) -> f64 {
        self.message_throughput.peak_rate()
    }
    
    /// Get work steal success rate
    pub fn steal_success_rate(&self) -> f64 {
        let attempts = self.steal_attempts.load(Ordering::Relaxed);
        if attempts == 0 {
            0.0
        } else {
            let successful = self.successful_steals.load(Ordering::Relaxed);
            successful as f64 / attempts as f64
        }
    }
    
    /// Get NUMA locality rate
    pub fn numa_locality_rate(&self) -> f64 {
        let hits = self.numa_locality_hits.load(Ordering::Relaxed);
        let misses = self.numa_locality_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
    
    /// Generate worker utilization report
    pub fn worker_utilization_report(&self) -> UtilizationReport {
        let mut workers = Vec::with_capacity(self.worker_count);
        let mut total_utilization = 0.0;
        let mut utilization_values = Vec::new();
        
        for worker_id in 0..self.worker_count {
            let cpu_util = if worker_id < self.worker_utilization.len() {
                self.worker_utilization[worker_id].load(Ordering::Relaxed) as f64 / 100.0
            } else {
                0.0
            };
            
            let task_count = if worker_id < self.worker_task_counts.len() {
                self.worker_task_counts[worker_id].load(Ordering::Relaxed)
            } else {
                0
            };
            
            let queue_depth = if worker_id < self.worker_queue_depths.len() {
                self.worker_queue_depths[worker_id].load(Ordering::Relaxed)
            } else {
                0
            };
            
            workers.push(WorkerUtilization {
                worker_id,
                cpu_utilization: cpu_util,
                task_rate: task_count as f64, // Simplified
                avg_task_time: self.average_latency(),
                steal_success_rate: self.steal_success_rate(),
                queue_depth,
            });
            
            total_utilization += cpu_util;
            utilization_values.push(cpu_util);
        }
        
        let system_utilization = if self.worker_count > 0 {
            total_utilization / self.worker_count as f64
        } else {
            0.0
        };
        
        // Calculate load balance score (coefficient of variation)
        let load_balance_score = if utilization_values.len() > 1 {
            let mean = system_utilization;
            let variance = utilization_values
                .iter()
                .map(|&x| (x - mean).powi(2))
                .sum::<f64>() / utilization_values.len() as f64;
            let std_dev = variance.sqrt();
            
            if mean > 0.0 {
                std_dev / mean // Coefficient of variation
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        UtilizationReport {
            workers,
            system_utilization,
            load_balance_score,
            timestamp: Utc::now(),
        }
    }
    
    /// Check if performance targets are being met
    pub fn performance_status(&self) -> PerformanceStatus {
        let p99_latency = self.p99_latency();
        let current_throughput = self.current_message_throughput() as u64;
        
        let latency_target_met = p99_latency <= self.target_latency;
        let throughput_target_met = current_throughput >= self.target_throughput;
        
        PerformanceStatus {
            latency_target_met,
            throughput_target_met,
            current_p99_latency: p99_latency,
            current_throughput,
            target_latency: self.target_latency,
            target_throughput: self.target_throughput,
            violation_rate: self.task_latency.violation_rate(),
        }
    }
    
    /// Reset all metrics
    pub fn reset(&self) {
        self.task_latency.reset();
        self.scheduling_overhead.reset();
        self.message_latency.reset();
        
        for worker_util in &self.worker_utilization {
            worker_util.store(0, Ordering::Relaxed);
        }
        
        for task_count in &self.worker_task_counts {
            task_count.store(0, Ordering::Relaxed);
        }
        
        for queue_depth in &self.worker_queue_depths {
            queue_depth.store(0, Ordering::Relaxed);
        }
        
        self.total_tasks_executed.store(0, Ordering::Relaxed);
        self.total_messages_processed.store(0, Ordering::Relaxed);
        self.steal_attempts.store(0, Ordering::Relaxed);
        self.successful_steals.store(0, Ordering::Relaxed);
        self.numa_locality_hits.store(0, Ordering::Relaxed);
        self.numa_locality_misses.store(0, Ordering::Relaxed);
    }
}

/// Performance status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStatus {
    /// Whether latency target is being met
    pub latency_target_met: bool,
    
    /// Whether throughput target is being met
    pub throughput_target_met: bool,
    
    /// Current P99 latency
    #[serde(with = "duration_serde")]
    pub current_p99_latency: Duration,
    
    /// Current throughput
    pub current_throughput: u64,
    
    /// Target latency
    #[serde(with = "duration_serde")]
    pub target_latency: Duration,
    
    /// Target throughput
    pub target_throughput: u64,
    
    /// Violation rate (0.0 to 1.0)
    pub violation_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_latency_histogram() {
        let histogram = LatencyHistogram::new(Duration::from_micros(10));
        
        // Test initial state
        assert_eq!(histogram.sample_count(), 0);
        assert_eq!(histogram.p99(), Duration::ZERO);
        assert_eq!(histogram.violations(), 0);
        
        // Record some latencies
        histogram.record(Duration::from_micros(1));
        histogram.record(Duration::from_micros(5));
        histogram.record(Duration::from_micros(15)); // Violation
        
        assert_eq!(histogram.sample_count(), 3);
        assert_eq!(histogram.violations(), 1);
        assert_eq!(histogram.violation_rate(), 1.0 / 3.0);
        
        // Test percentiles
        assert!(histogram.p50() > Duration::ZERO);
        assert!(histogram.p99() > histogram.p50());
        
        // Test reset
        histogram.reset();
        assert_eq!(histogram.sample_count(), 0);
        assert_eq!(histogram.violations(), 0);
    }
    
    #[test]
    fn test_throughput_meter() {
        let meter = ThroughputMeter::new(Duration::from_millis(100), 5);
        
        // Record some events
        meter.record(10);
        meter.record(5);
        
        // Should have some rate
        assert!(meter.current_rate() > 0.0);
        assert!(meter.average_rate() > 0.0);
        assert!(meter.peak_rate() > 0.0);
        
        // Test window advancement (this is time-dependent)
        thread::sleep(Duration::from_millis(110));
        meter.record(20);
        
        // Should still have rates
        assert!(meter.average_rate() > 0.0);
    }
    
    #[test]
    fn test_runtime_metrics() {
        let metrics = RuntimeMetrics::new(
            Duration::from_micros(10),
            1000000,
            4,
        );
        
        // Test initial state
        assert_eq!(metrics.p99_latency(), Duration::ZERO);
        assert_eq!(metrics.current_task_throughput(), 0.0);
        assert_eq!(metrics.steal_success_rate(), 0.0);
        assert_eq!(metrics.numa_locality_rate(), 0.0);
        
        // Record some metrics
        metrics.record_task_latency(Duration::from_micros(5));
        metrics.record_message_latency(Duration::from_micros(8));
        metrics.record_steal_attempt(true);
        metrics.record_steal_attempt(false);
        metrics.record_numa_locality(true);
        
        // Verify metrics
        assert!(metrics.p99_latency() > Duration::ZERO);
        assert!(metrics.current_task_throughput() > 0.0);
        assert_eq!(metrics.steal_success_rate(), 0.5);
        assert_eq!(metrics.numa_locality_rate(), 1.0);
        
        // Test worker utilization
        metrics.update_worker_utilization(0, 75);
        metrics.update_worker_queue_depth(0, 10);
        
        let report = metrics.worker_utilization_report();
        assert_eq!(report.workers.len(), 4);
        assert_eq!(report.workers[0].cpu_utilization, 0.75);
        assert_eq!(report.workers[0].queue_depth, 10);
        
        // Test performance status
        let status = metrics.performance_status();
        assert!(status.latency_target_met); // 5μs < 10μs target
        
        // Test reset
        metrics.reset();
        assert_eq!(metrics.p99_latency(), Duration::ZERO);
        assert_eq!(metrics.steal_success_rate(), 0.0);
    }
    
    #[test]
    fn test_worker_utilization_report() {
        let metrics = RuntimeMetrics::new(
            Duration::from_micros(10),
            1000000,
            2,
        );
        
        // Set different utilizations
        metrics.update_worker_utilization(0, 80);
        metrics.update_worker_utilization(1, 60);
        
        let report = metrics.worker_utilization_report();
        assert_eq!(report.workers.len(), 2);
        assert_eq!(report.system_utilization, 0.7); // (0.8 + 0.6) / 2
        assert!(report.load_balance_score > 0.0); // Some imbalance
    }
    
    #[test]
    fn test_performance_status() {
        let metrics = RuntimeMetrics::new(
            Duration::from_micros(10),
            1000,
            1,
        );
        
        // Record latency within target
        metrics.record_task_latency(Duration::from_micros(5));
        metrics.record_message_latency(Duration::from_micros(5));
        
        let status = metrics.performance_status();
        assert!(status.latency_target_met);
        assert_eq!(status.current_p99_latency, Duration::from_micros(5));
        assert_eq!(status.target_latency, Duration::from_micros(10));
    }
}