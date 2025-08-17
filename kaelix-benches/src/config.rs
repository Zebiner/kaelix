//! Benchmark configuration and environment setup.

use std::time::Duration;
use std::env;

/// Configuration for benchmark execution environment.
#[derive(Debug, Clone)]
pub struct BenchmarkEnvironment {
    /// Number of CPU cores to use
    pub cpu_cores: usize,
    /// Memory limit in bytes
    pub memory_limit: Option<u64>,
    /// Network bandwidth limit in bytes/sec
    pub network_bandwidth: Option<u64>,
    /// Disk I/O limit in bytes/sec
    pub disk_io_limit: Option<u64>,
    /// Enable NUMA optimization
    pub numa_enabled: bool,
    /// CPU affinity settings
    pub cpu_affinity: Option<Vec<usize>>,
}

impl Default for BenchmarkEnvironment {
    fn default() -> Self {
        Self {
            cpu_cores: num_cpus::get(),
            memory_limit: None,
            network_bandwidth: None,
            disk_io_limit: None,
            numa_enabled: false,
            cpu_affinity: None,
        }
    }
}

impl BenchmarkEnvironment {
    /// Create optimized environment for high-performance benchmarks.
    pub fn high_performance() -> Self {
        Self {
            cpu_cores: num_cpus::get(),
            memory_limit: None,
            network_bandwidth: None,
            disk_io_limit: None,
            numa_enabled: true,
            cpu_affinity: Some((0..num_cpus::get()).collect()),
        }
    }

    /// Create resource-constrained environment for stress testing.
    pub fn constrained() -> Self {
        let cores = std::cmp::max(1, num_cpus::get() / 2);
        Self {
            cpu_cores: cores,
            memory_limit: Some(1024 * 1024 * 1024), // 1GB
            network_bandwidth: Some(100 * 1024 * 1024), // 100MB/s
            disk_io_limit: Some(50 * 1024 * 1024), // 50MB/s
            numa_enabled: false,
            cpu_affinity: Some((0..cores).collect()),
        }
    }

    /// Apply environment configuration.
    pub fn apply(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Set CPU affinity if specified
        if let Some(ref affinity) = self.cpu_affinity {
            env::set_var("TOKIO_WORKER_THREADS", &self.cpu_cores.to_string());
            tracing::info!("Set CPU affinity to cores: {:?}", affinity);
        }

        // Configure memory limits
        if let Some(limit) = self.memory_limit {
            // Note: Actual memory limiting would require system-level configuration
            tracing::info!("Memory limit set to: {} bytes", limit);
        }

        // Configure NUMA if enabled
        if self.numa_enabled {
            env::set_var("NUMA_OPTIMIZE", "1");
            tracing::info!("NUMA optimization enabled");
        }

        Ok(())
    }
}

/// Benchmark scenario configuration.
#[derive(Debug, Clone)]
pub struct BenchmarkScenario {
    /// Scenario name
    pub name: String,
    /// Load pattern
    pub load_pattern: LoadPattern,
    /// Duration of the benchmark
    pub duration: Duration,
    /// Ramp-up period
    pub ramp_up: Duration,
    /// Cool-down period
    pub cool_down: Duration,
    /// Expected performance characteristics
    pub performance_baseline: PerformanceBaseline,
}

/// Load pattern for benchmark execution.
#[derive(Debug, Clone)]
pub enum LoadPattern {
    /// Constant load throughout the test
    Constant {
        /// Target messages per second
        target_rate: u64,
    },
    /// Gradually increasing load
    Ramp {
        /// Starting rate
        start_rate: u64,
        /// Ending rate
        end_rate: u64,
    },
    /// Spike pattern with sudden load increases
    Spike {
        /// Base load rate
        base_rate: u64,
        /// Spike rate
        spike_rate: u64,
        /// Spike duration
        spike_duration: Duration,
        /// Interval between spikes
        spike_interval: Duration,
    },
    /// Burst pattern with periods of high activity
    Burst {
        /// Burst rate
        burst_rate: u64,
        /// Quiet rate
        quiet_rate: u64,
        /// Burst duration
        burst_duration: Duration,
        /// Quiet duration
        quiet_duration: Duration,
    },
}

impl LoadPattern {
    /// Calculate the target rate at a given time offset.
    pub fn rate_at_time(&self, elapsed: Duration, total_duration: Duration) -> u64 {
        match self {
            LoadPattern::Constant { target_rate } => *target_rate,
            LoadPattern::Ramp { start_rate, end_rate } => {
                let progress = elapsed.as_secs_f64() / total_duration.as_secs_f64();
                let progress = progress.clamp(0.0, 1.0);
                let rate_diff = *end_rate as f64 - *start_rate as f64;
                (*start_rate as f64 + rate_diff * progress) as u64
            }
            LoadPattern::Spike { base_rate, spike_rate, spike_duration, spike_interval } => {
                let cycle_duration = *spike_duration + *spike_interval;
                let cycle_position = elapsed.as_nanos() % cycle_duration.as_nanos();
                
                if cycle_position < spike_duration.as_nanos() {
                    *spike_rate
                } else {
                    *base_rate
                }
            }
            LoadPattern::Burst { burst_rate, quiet_rate, burst_duration, quiet_duration } => {
                let cycle_duration = *burst_duration + *quiet_duration;
                let cycle_position = elapsed.as_nanos() % cycle_duration.as_nanos();
                
                if cycle_position < burst_duration.as_nanos() {
                    *burst_rate
                } else {
                    *quiet_rate
                }
            }
        }
    }
}

/// Performance baseline for comparison.
#[derive(Debug, Clone)]
pub struct PerformanceBaseline {
    /// Expected minimum throughput
    pub min_throughput: u64,
    /// Expected maximum latency
    pub max_latency: Duration,
    /// Expected P99 latency
    pub p99_latency: Duration,
    /// Expected CPU utilization
    pub cpu_utilization: f64,
    /// Expected memory usage
    pub memory_usage: u64,
    /// Error rate threshold
    pub max_error_rate: f64,
}

impl PerformanceBaseline {
    /// Create baseline for high-throughput scenarios.
    pub fn high_throughput() -> Self {
        Self {
            min_throughput: 10_000_000,
            max_latency: Duration::from_micros(50),
            p99_latency: Duration::from_micros(10),
            cpu_utilization: 80.0,
            memory_usage: 1024 * 1024 * 1024, // 1GB
            max_error_rate: 0.001, // 0.1%
        }
    }

    /// Create baseline for low-latency scenarios.
    pub fn low_latency() -> Self {
        Self {
            min_throughput: 100_000,
            max_latency: Duration::from_micros(10),
            p99_latency: Duration::from_micros(5),
            cpu_utilization: 60.0,
            memory_usage: 512 * 1024 * 1024, // 512MB
            max_error_rate: 0.0001, // 0.01%
        }
    }

    /// Create baseline for balanced scenarios.
    pub fn balanced() -> Self {
        Self {
            min_throughput: 1_000_000,
            max_latency: Duration::from_micros(20),
            p99_latency: Duration::from_micros(10),
            cpu_utilization: 70.0,
            memory_usage: 512 * 1024 * 1024, // 512MB
            max_error_rate: 0.01, // 1%
        }
    }
}

/// Standard benchmark scenarios.
pub struct BenchmarkScenarios;

impl BenchmarkScenarios {
    /// Sustained high-throughput scenario.
    pub fn sustained_throughput() -> BenchmarkScenario {
        BenchmarkScenario {
            name: "sustained_throughput".to_string(),
            load_pattern: LoadPattern::Constant { target_rate: 10_000_000 },
            duration: Duration::from_secs(60),
            ramp_up: Duration::from_secs(10),
            cool_down: Duration::from_secs(5),
            performance_baseline: PerformanceBaseline::high_throughput(),
        }
    }

    /// Low-latency precision scenario.
    pub fn low_latency() -> BenchmarkScenario {
        BenchmarkScenario {
            name: "low_latency".to_string(),
            load_pattern: LoadPattern::Constant { target_rate: 100_000 },
            duration: Duration::from_secs(30),
            ramp_up: Duration::from_secs(5),
            cool_down: Duration::from_secs(2),
            performance_baseline: PerformanceBaseline::low_latency(),
        }
    }

    /// Load spike resilience scenario.
    pub fn spike_test() -> BenchmarkScenario {
        BenchmarkScenario {
            name: "spike_test".to_string(),
            load_pattern: LoadPattern::Spike {
                base_rate: 1_000_000,
                spike_rate: 10_000_000,
                spike_duration: Duration::from_secs(2),
                spike_interval: Duration::from_secs(8),
            },
            duration: Duration::from_secs(60),
            ramp_up: Duration::from_secs(5),
            cool_down: Duration::from_secs(5),
            performance_baseline: PerformanceBaseline::balanced(),
        }
    }

    /// Gradual load increase scenario.
    pub fn ramp_test() -> BenchmarkScenario {
        BenchmarkScenario {
            name: "ramp_test".to_string(),
            load_pattern: LoadPattern::Ramp {
                start_rate: 100_000,
                end_rate: 5_000_000,
            },
            duration: Duration::from_secs(120),
            ramp_up: Duration::from_secs(0),
            cool_down: Duration::from_secs(10),
            performance_baseline: PerformanceBaseline::balanced(),
        }
    }

    /// Burst load scenario.
    pub fn burst_test() -> BenchmarkScenario {
        BenchmarkScenario {
            name: "burst_test".to_string(),
            load_pattern: LoadPattern::Burst {
                burst_rate: 5_000_000,
                quiet_rate: 100_000,
                burst_duration: Duration::from_secs(5),
                quiet_duration: Duration::from_secs(15),
            },
            duration: Duration::from_secs(120),
            ramp_up: Duration::from_secs(5),
            cool_down: Duration::from_secs(5),
            performance_baseline: PerformanceBaseline::balanced(),
        }
    }

    /// Get all standard scenarios.
    pub fn all() -> Vec<BenchmarkScenario> {
        vec![
            Self::sustained_throughput(),
            Self::low_latency(),
            Self::spike_test(),
            Self::ramp_test(),
            Self::burst_test(),
        ]
    }
}

/// Configuration for benchmark warmup.
#[derive(Debug, Clone)]
pub struct WarmupConfig {
    /// Duration of warmup phase
    pub duration: Duration,
    /// Target rate during warmup
    pub target_rate: u64,
    /// Number of warmup iterations
    pub iterations: usize,
}

impl Default for WarmupConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(10),
            target_rate: 1000,
            iterations: 100,
        }
    }
}

impl WarmupConfig {
    /// Create warmup config for high-performance scenarios.
    pub fn high_performance() -> Self {
        Self {
            duration: Duration::from_secs(15),
            target_rate: 100_000,
            iterations: 1000,
        }
    }

    /// Create warmup config for latency-sensitive scenarios.
    pub fn latency_optimized() -> Self {
        Self {
            duration: Duration::from_secs(5),
            target_rate: 10_000,
            iterations: 500,
        }
    }
}