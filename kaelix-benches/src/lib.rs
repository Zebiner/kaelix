//! # Kaelix Benchmarks
//!
//! Performance benchmarks for the MemoryStreamer distributed streaming system.
//!
//! This crate provides comprehensive benchmarks for:
//! - Message throughput testing
//! - Latency measurements
//! - Memory usage profiling
//! - Concurrent operation performance
//! - End-to-end system benchmarks
//!
//! ## Benchmark Categories
//!
//! ### Microbenchmarks
//! - Individual component performance
//! - Data structure operations
//! - Serialization/deserialization
//! - Memory allocation patterns
//!
//! ### Integration Benchmarks
//! - End-to-end message flow
//! - Multi-component interactions
//! - Network communication overhead
//! - Storage layer performance
//!
//! ### Load Benchmarks
//! - High-throughput scenarios
//! - Sustained load testing
//! - Scalability measurements
//! - Resource utilization under load
//!
//! ## Performance Targets
//!
//! The benchmarks validate against these targets:
//! - **Throughput**: 10M+ messages/second
//! - **Latency**: <10μs P99 latency
//! - **Memory**: Efficient memory usage with minimal allocations
//! - **CPU**: Optimal CPU utilization
//!
//! ## Running Benchmarks
//!
//! ```bash
//! # Run all benchmarks
//! cargo bench
//!
//! # Run specific benchmark
//! cargo bench message_throughput
//!
//! # Generate flamegraph
//! cargo bench --features pprof
//!
//! # Memory profiling
//! cargo bench --features dhat
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

pub mod setup;
pub mod utils;
pub mod config;
pub mod metrics;
pub mod validators;
pub mod regression;

use std::time::Duration;
use kaelix_core::Result;

/// Configurable benchmark parameters and settings.
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Target throughput in messages per second
    pub target_throughput: u64,
    /// Maximum acceptable latency
    pub max_latency: Duration,
    /// Number of warmup iterations
    pub warmup_iterations: usize,
    /// Number of measurement iterations
    pub measurement_iterations: usize,
    /// Confidence threshold for statistical significance
    pub confidence_threshold: f64,
    /// Whether to enable memory profiling
    pub enable_memory_profiling: bool,
    /// Whether to enable CPU profiling
    pub enable_cpu_profiling: bool,
    /// Regression detection threshold (percentage)
    pub regression_threshold: f64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            target_throughput: 10_000_000, // 10M msg/sec
            max_latency: Duration::from_micros(10), // <10μs P99
            warmup_iterations: 100,
            measurement_iterations: 1000,
            confidence_threshold: 0.95,
            enable_memory_profiling: false,
            enable_cpu_profiling: false,
            regression_threshold: 1.0, // 1% threshold
        }
    }
}

impl BenchmarkConfig {
    /// Create configuration for throughput benchmarks.
    pub fn throughput() -> Self {
        Self {
            target_throughput: 10_000_000,
            max_latency: Duration::from_micros(50),
            measurement_iterations: 500,
            ..Self::default()
        }
    }

    /// Create configuration for latency benchmarks.
    pub fn latency() -> Self {
        Self {
            target_throughput: 100_000,
            max_latency: Duration::from_micros(5),
            measurement_iterations: 2000,
            ..Self::default()
        }
    }

    /// Create configuration for memory benchmarks.
    pub fn memory() -> Self {
        Self {
            target_throughput: 1_000_000,
            max_latency: Duration::from_micros(20),
            enable_memory_profiling: true,
            measurement_iterations: 300,
            ..Self::default()
        }
    }

    /// Create configuration for concurrent benchmarks.
    pub fn concurrent() -> Self {
        Self {
            target_throughput: 5_000_000,
            max_latency: Duration::from_micros(15),
            measurement_iterations: 800,
            ..Self::default()
        }
    }

    /// Create configuration for stress testing.
    pub fn stress() -> Self {
        Self {
            target_throughput: 15_000_000,
            max_latency: Duration::from_micros(25),
            measurement_iterations: 1500,
            enable_cpu_profiling: true,
            ..Self::default()
        }
    }
}

/// Performance validator for ensuring benchmarks meet targets.
pub struct PerformanceValidator {
    config: BenchmarkConfig,
}

impl PerformanceValidator {
    /// Create a new performance validator with the given configuration.
    pub fn new(config: BenchmarkConfig) -> Self {
        Self { config }
    }

    /// Validate throughput meets targets.
    pub fn validate_throughput(&self, measured_throughput: u64) -> Result<()> {
        if measured_throughput >= self.config.target_throughput {
            tracing::info!(
                "Throughput validation passed: {} >= {} msg/sec",
                measured_throughput,
                self.config.target_throughput
            );
            Ok(())
        } else {
            Err(kaelix_core::Error::Performance(format!(
                "Throughput validation failed: {} < {} msg/sec",
                measured_throughput, self.config.target_throughput
            )))
        }
    }

    /// Validate latency meets targets.
    pub fn validate_latency(&self, measured_latency: Duration) -> Result<()> {
        if measured_latency <= self.config.max_latency {
            tracing::info!(
                "Latency validation passed: {:?} <= {:?}",
                measured_latency,
                self.config.max_latency
            );
            Ok(())
        } else {
            Err(kaelix_core::Error::Performance(format!(
                "Latency validation failed: {:?} > {:?}",
                measured_latency, self.config.max_latency
            )))
        }
    }

    /// Validate P99 latency specifically.
    pub fn validate_p99_latency(&self, p99_latency: Duration) -> Result<()> {
        let p99_target = self.config.max_latency;
        if p99_latency <= p99_target {
            tracing::info!(
                "P99 latency validation passed: {:?} <= {:?}",
                p99_latency,
                p99_target
            );
            Ok(())
        } else {
            Err(kaelix_core::Error::Performance(format!(
                "P99 latency validation failed: {:?} > {:?}",
                p99_latency, p99_target
            )))
        }
    }
}

/// Comprehensive benchmark metrics tracking.
#[derive(Debug, Clone)]
pub struct BenchmarkMetrics {
    /// Throughput in messages per second
    pub throughput: u64,
    /// Latency statistics
    pub latency_stats: utils::latency::LatencyStats,
    /// Memory usage metrics
    pub memory_usage: MemoryMetrics,
    /// CPU utilization percentage
    pub cpu_utilization: f64,
    /// Test duration
    pub duration: Duration,
    /// Number of messages processed
    pub message_count: u64,
    /// Error count
    pub error_count: u64,
}

/// Memory usage metrics.
#[derive(Debug, Clone, Default)]
pub struct MemoryMetrics {
    /// Peak memory usage in bytes
    pub peak_memory: u64,
    /// Average memory usage in bytes
    pub average_memory: u64,
    /// Total allocations
    pub total_allocations: u64,
    /// Total deallocations
    pub total_deallocations: u64,
    /// Memory leaks (allocations - deallocations)
    pub memory_leaks: i64,
}

impl BenchmarkMetrics {
    /// Create new metrics with zero values.
    pub fn new() -> Self {
        Self {
            throughput: 0,
            latency_stats: utils::latency::LatencyStats::default(),
            memory_usage: MemoryMetrics::default(),
            cpu_utilization: 0.0,
            duration: Duration::ZERO,
            message_count: 0,
            error_count: 0,
        }
    }

    /// Calculate throughput from message count and duration.
    pub fn calculate_throughput(&mut self) {
        if !self.duration.is_zero() {
            let seconds = self.duration.as_secs_f64();
            self.throughput = (self.message_count as f64 / seconds) as u64;
        }
    }

    /// Add error to error count.
    pub fn add_error(&mut self) {
        self.error_count += 1;
    }

    /// Calculate success rate.
    pub fn success_rate(&self) -> f64 {
        if self.message_count == 0 {
            return 0.0;
        }
        let successful = self.message_count.saturating_sub(self.error_count);
        successful as f64 / self.message_count as f64
    }

    /// Print comprehensive metrics summary.
    pub fn print_summary(&self) {
        println!("\n=== Benchmark Metrics Summary ===");
        println!("Throughput: {} msg/sec", self.throughput);
        println!("Messages processed: {}", self.message_count);
        println!("Errors: {}", self.error_count);
        println!("Success rate: {:.2}%", self.success_rate() * 100.0);
        println!("Duration: {:?}", self.duration);
        println!("CPU utilization: {:.2}%", self.cpu_utilization);
        println!("\nLatency Statistics:");
        self.latency_stats.print_summary();
        println!("\nMemory Statistics:");
        println!("  Peak memory: {} bytes", self.memory_usage.peak_memory);
        println!("  Average memory: {} bytes", self.memory_usage.average_memory);
        println!("  Total allocations: {}", self.memory_usage.total_allocations);
        println!("  Memory leaks: {}", self.memory_usage.memory_leaks);
        println!("=================================\n");
    }
}

impl Default for BenchmarkMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Async benchmark utilities for tokio integration.
pub mod async_utils {
    use super::*;
    use std::future::Future;
    use tokio::time::Instant;

    /// Run an async benchmark with the given operation.
    pub async fn benchmark_async<F, Fut, T>(
        name: &str,
        iterations: usize,
        mut op: F,
    ) -> (BenchmarkMetrics, Vec<T>)
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T>>,
        T: std::fmt::Debug,
    {
        let mut metrics = BenchmarkMetrics::new();
        let mut results = Vec::with_capacity(iterations);
        let mut latencies = Vec::with_capacity(iterations);

        tracing::info!("Starting async benchmark: {}", name);
        let start_time = Instant::now();

        for i in 0..iterations {
            if i % 100 == 0 {
                tracing::debug!("Benchmark progress: {}/{}", i, iterations);
            }

            let op_start = Instant::now();
            match op().await {
                Ok(result) => {
                    let latency = op_start.elapsed();
                    latencies.push(latency);
                    results.push(result);
                    metrics.message_count += 1;
                }
                Err(e) => {
                    tracing::warn!("Benchmark operation failed: {:?}", e);
                    metrics.add_error();
                }
            }
        }

        metrics.duration = start_time.elapsed();
        metrics.latency_stats = utils::latency::LatencyStats::from_measurements(latencies);
        metrics.calculate_throughput();

        tracing::info!("Completed async benchmark: {}", name);
        (metrics, results)
    }

    /// Run an async batch benchmark.
    pub async fn benchmark_async_batch<F, Fut, T>(
        name: &str,
        batch_size: usize,
        batch_count: usize,
        mut op: F,
    ) -> BenchmarkMetrics
    where
        F: FnMut(usize) -> Fut,
        Fut: Future<Output = Result<Vec<T>>>,
        T: std::fmt::Debug,
    {
        let mut metrics = BenchmarkMetrics::new();
        let mut latencies = Vec::with_capacity(batch_count);

        tracing::info!("Starting async batch benchmark: {} ({}x{})", name, batch_count, batch_size);
        let start_time = Instant::now();

        for i in 0..batch_count {
            if i % 10 == 0 {
                tracing::debug!("Batch progress: {}/{}", i, batch_count);
            }

            let batch_start = Instant::now();
            match op(batch_size).await {
                Ok(batch_results) => {
                    let latency = batch_start.elapsed();
                    latencies.push(latency);
                    metrics.message_count += batch_results.len() as u64;
                }
                Err(e) => {
                    tracing::warn!("Batch operation failed: {:?}", e);
                    metrics.add_error();
                }
            }
        }

        metrics.duration = start_time.elapsed();
        metrics.latency_stats = utils::latency::LatencyStats::from_measurements(latencies);
        metrics.calculate_throughput();

        tracing::info!("Completed async batch benchmark: {}", name);
        metrics
    }

    /// Run concurrent async benchmark with multiple tasks.
    pub async fn benchmark_concurrent<F, Fut, T>(
        name: &str,
        concurrency: usize,
        iterations_per_task: usize,
        op: F,
    ) -> BenchmarkMetrics
    where
        F: Fn() -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Result<T>> + Send,
        T: std::fmt::Debug + Send,
    {
        let mut metrics = BenchmarkMetrics::new();
        
        tracing::info!("Starting concurrent benchmark: {} ({}x{})", name, concurrency, iterations_per_task);
        let start_time = Instant::now();

        let mut tasks = Vec::new();
        for task_id in 0..concurrency {
            let op_clone = op.clone();
            let task = tokio::spawn(async move {
                let mut task_metrics = BenchmarkMetrics::new();
                let mut latencies = Vec::with_capacity(iterations_per_task);

                for _ in 0..iterations_per_task {
                    let op_start = Instant::now();
                    match op_clone().await {
                        Ok(_) => {
                            let latency = op_start.elapsed();
                            latencies.push(latency);
                            task_metrics.message_count += 1;
                        }
                        Err(e) => {
                            tracing::warn!("Task {} operation failed: {:?}", task_id, e);
                            task_metrics.add_error();
                        }
                    }
                }

                task_metrics.latency_stats = utils::latency::LatencyStats::from_measurements(latencies);
                task_metrics
            });
            tasks.push(task);
        }

        // Wait for all tasks to complete
        let task_results = futures::future::join_all(tasks).await;
        
        // Aggregate results
        for task_result in task_results {
            match task_result {
                Ok(task_metrics) => {
                    metrics.message_count += task_metrics.message_count;
                    metrics.error_count += task_metrics.error_count;
                }
                Err(e) => {
                    tracing::error!("Task failed to complete: {:?}", e);
                }
            }
        }

        metrics.duration = start_time.elapsed();
        metrics.calculate_throughput();

        tracing::info!("Completed concurrent benchmark: {}", name);
        metrics
    }
}

/// Performance benchmark utilities and helpers.
pub mod prelude {
    pub use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
    pub use kaelix_core::prelude::*;
    pub use kaelix_tests::prelude::*;
    pub use crate::{setup::*, utils::*, config::*, metrics::*, validators::*, regression::*};
    pub use crate::{BenchmarkConfig, PerformanceValidator, BenchmarkMetrics, MemoryMetrics};
    pub use crate::async_utils;
    pub use std::time::Duration;
    pub use tokio::time::Instant;
}