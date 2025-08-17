//! Utilities for benchmark measurements and analysis.

use criterion::{Criterion, BenchmarkId, Throughput};
use std::time::{Duration, Instant};

/// Utilities for measuring throughput.
pub mod throughput {
    use super::*;

    /// Measure operations per second for a given function.
    pub fn measure_ops_per_sec<F>(criterion: &mut Criterion, name: &str, op: F)
    where
        F: Fn() + Copy,
    {
        criterion.bench_function(name, |b| {
            b.iter(|| {
                criterion::black_box(op());
            });
        });
    }

    /// Measure throughput for batch operations.
    pub fn measure_batch_throughput<F>(
        criterion: &mut Criterion,
        name: &str,
        batch_sizes: &[usize],
        op: F,
    )
    where
        F: Fn(usize) + Copy,
    {
        let mut group = criterion.benchmark_group(name);
        
        for &size in batch_sizes {
            group.throughput(Throughput::Elements(size as u64));
            group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
                b.iter(|| {
                    criterion::black_box(op(size));
                });
            });
        }
        
        group.finish();
    }

    /// Measure throughput with different payload sizes.
    pub fn measure_payload_throughput<F>(
        criterion: &mut Criterion,
        name: &str,
        payload_sizes: &[usize],
        op: F,
    )
    where
        F: Fn(usize) + Copy,
    {
        let mut group = criterion.benchmark_group(name);
        
        for &size in payload_sizes {
            group.throughput(Throughput::Bytes(size as u64));
            group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
                b.iter(|| {
                    criterion::black_box(op(size));
                });
            });
        }
        
        group.finish();
    }
}

/// Utilities for measuring latency.
pub mod latency {
    use super::*;

    /// Measure end-to-end latency for an operation.
    pub fn measure_latency<F, T>(criterion: &mut Criterion, name: &str, op: F)
    where
        F: Fn() -> T + Copy,
        T: std::fmt::Debug,
    {
        criterion.bench_function(name, |b| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                for _ in 0..iters {
                    criterion::black_box(op());
                }
                start.elapsed()
            });
        });
    }

    /// Measure latency percentiles for an operation.
    pub fn measure_latency_percentiles<F, T>(
        name: &str,
        iterations: usize,
        op: F,
    ) -> LatencyStats
    where
        F: Fn() -> T,
        T: std::fmt::Debug,
    {
        let mut measurements = Vec::with_capacity(iterations);
        
        for _ in 0..iterations {
            let start = Instant::now();
            criterion::black_box(op());
            measurements.push(start.elapsed());
        }
        
        LatencyStats::from_measurements(measurements)
    }

    /// Latency statistics.
    #[derive(Debug, Clone)]
    pub struct LatencyStats {
        /// Minimum latency
        pub min: Duration,
        /// Maximum latency
        pub max: Duration,
        /// Mean latency
        pub mean: Duration,
        /// Median latency (P50)
        pub p50: Duration,
        /// 95th percentile
        pub p95: Duration,
        /// 99th percentile
        pub p99: Duration,
        /// 99.9th percentile
        pub p999: Duration,
    }

    impl LatencyStats {
        /// Calculate statistics from a vector of measurements.
        pub fn from_measurements(mut measurements: Vec<Duration>) -> Self {
            if measurements.is_empty() {
                return Self::default();
            }

            measurements.sort_unstable();
            let len = measurements.len();

            let min = measurements[0];
            let max = measurements[len - 1];
            
            let sum: Duration = measurements.iter().sum();
            let mean = sum / len as u32;
            
            let p50 = measurements[len / 2];
            let p95 = measurements[len * 95 / 100];
            let p99 = measurements[len * 99 / 100];
            let p999 = measurements[len * 999 / 1000];

            Self {
                min,
                max,
                mean,
                p50,
                p95,
                p99,
                p999,
            }
        }

        /// Print a summary of the latency statistics.
        pub fn print_summary(&self) {
            println!("Latency Statistics:");
            println!("  Min:   {:?}", self.min);
            println!("  Max:   {:?}", self.max);
            println!("  Mean:  {:?}", self.mean);
            println!("  P50:   {:?}", self.p50);
            println!("  P95:   {:?}", self.p95);
            println!("  P99:   {:?}", self.p99);
            println!("  P99.9: {:?}", self.p999);
        }
    }

    impl Default for LatencyStats {
        fn default() -> Self {
            Self {
                min: Duration::ZERO,
                max: Duration::ZERO,
                mean: Duration::ZERO,
                p50: Duration::ZERO,
                p95: Duration::ZERO,
                p99: Duration::ZERO,
                p999: Duration::ZERO,
            }
        }
    }
}

/// Utilities for memory measurement.
pub mod memory {
    use super::*;

    /// Measure memory allocation for an operation.
    #[cfg(feature = "dhat")]
    pub fn measure_allocations<F, T>(name: &str, op: F) -> T
    where
        F: FnOnce() -> T,
    {
        let _profiler = dhat::Profiler::new_heap();
        
        println!("Starting memory profiling for: {}", name);
        let result = op();
        println!("Memory profiling completed for: {}", name);
        
        result
    }

    #[cfg(not(feature = "dhat"))]
    pub fn measure_allocations<F, T>(_name: &str, op: F) -> T
    where
        F: FnOnce() -> T,
    {
        op()
    }

    /// Simple memory usage tracker.
    pub struct MemoryTracker {
        initial: usize,
    }

    impl MemoryTracker {
        /// Start tracking memory usage.
        pub fn start() -> Self {
            Self {
                initial: Self::current_usage(),
            }
        }

        /// Get memory usage since tracking started.
        pub fn usage_since_start(&self) -> isize {
            Self::current_usage() as isize - self.initial as isize
        }

        /// Get current memory usage (simplified).
        fn current_usage() -> usize {
            // This is a simplified implementation
            // In practice, you'd use a more sophisticated memory tracking mechanism
            0
        }
    }
}

/// Utilities for CPU profiling.
pub mod profiling {
    use super::*;

    /// Run a benchmark with CPU profiling enabled.
    #[cfg(feature = "pprof")]
    pub fn with_cpu_profiling<F, T>(name: &str, op: F) -> T
    where
        F: FnOnce() -> T,
    {
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(1000)
            .blocklist(&["libc", "libgcc", "pthread", "vdso"])
            .build()
            .expect("Failed to create profiler");

        println!("Starting CPU profiling for: {}", name);
        let result = op();
        println!("CPU profiling completed for: {}", name);

        if let Ok(report) = guard.report().build() {
            let filename = format!("target/benchmark-{}.svg", name);
            let file = std::fs::File::create(&filename).expect("Failed to create profile file");
            let mut options = pprof::flamegraph::Options::default();
            options.image_width = Some(2500);
            report.flamegraph_with_options(file, &mut options).expect("Failed to write flamegraph");
            println!("Flamegraph saved to: {}", filename);
        }

        result
    }

    #[cfg(not(feature = "pprof"))]
    pub fn with_cpu_profiling<F, T>(_name: &str, op: F) -> T
    where
        F: FnOnce() -> T,
    {
        op()
    }

    /// Simple CPU usage tracker.
    pub struct CpuTracker {
        start_time: Instant,
    }

    impl CpuTracker {
        /// Start tracking CPU usage.
        pub fn start() -> Self {
            Self {
                start_time: Instant::now(),
            }
        }

        /// Get elapsed time since tracking started.
        pub fn elapsed(&self) -> Duration {
            self.start_time.elapsed()
        }
    }
}

/// Common benchmark patterns and utilities.
pub mod patterns {
    use super::*;

    /// Standard batch sizes for throughput testing.
    pub const BATCH_SIZES: &[usize] = &[1, 10, 100, 1000, 10000];

    /// Standard payload sizes for testing (in bytes).
    pub const PAYLOAD_SIZES: &[usize] = &[64, 256, 1024, 4096, 16384, 65536];

    /// Standard concurrency levels for testing.
    pub const CONCURRENCY_LEVELS: &[usize] = &[1, 2, 4, 8, 16, 32, 64];

    /// Run a benchmark with different batch sizes.
    pub fn benchmark_batch_sizes<F>(criterion: &mut Criterion, name: &str, op: F)
    where
        F: Fn(usize) + Copy,
    {
        throughput::measure_batch_throughput(criterion, name, BATCH_SIZES, op);
    }

    /// Run a benchmark with different payload sizes.
    pub fn benchmark_payload_sizes<F>(criterion: &mut Criterion, name: &str, op: F)
    where
        F: Fn(usize) + Copy,
    {
        throughput::measure_payload_throughput(criterion, name, PAYLOAD_SIZES, op);
    }

    /// Run a benchmark with different concurrency levels.
    pub fn benchmark_concurrency_levels<F>(criterion: &mut Criterion, name: &str, op: F)
    where
        F: Fn(usize) + Copy,
    {
        let mut group = criterion.benchmark_group(name);
        
        for &level in CONCURRENCY_LEVELS {
            group.bench_with_input(
                BenchmarkId::from_parameter(level), 
                &level, 
                |b, &level| {
                    b.iter(|| {
                        criterion::black_box(op(level));
                    });
                }
            );
        }
        
        group.finish();
    }
}