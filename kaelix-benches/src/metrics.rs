//! Performance metrics collection and analysis.

use std::time::{Duration, Instant};
use std::collections::HashMap;
use crate::{BenchmarkMetrics, MemoryMetrics};

/// Metrics collector for real-time performance tracking.
pub struct MetricsCollector {
    /// Start time of metrics collection
    start_time: Instant,
    /// Collected latency measurements
    latencies: Vec<Duration>,
    /// Message count per time window
    throughput_samples: Vec<(Instant, u64)>,
    /// Memory snapshots
    memory_snapshots: Vec<(Instant, MemorySnapshot)>,
    /// CPU usage samples
    cpu_samples: Vec<(Instant, f64)>,
    /// Error count
    error_count: u64,
    /// Total message count
    message_count: u64,
    /// Current time window for throughput calculation
    window_size: Duration,
}

/// Memory usage snapshot.
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// Heap usage in bytes
    pub heap_usage: u64,
    /// Stack usage in bytes
    pub stack_usage: u64,
    /// Total allocations since start
    pub total_allocations: u64,
    /// Total deallocations since start
    pub total_deallocations: u64,
    /// RSS memory in bytes
    pub rss_memory: u64,
}

impl Default for MemorySnapshot {
    fn default() -> Self {
        Self {
            heap_usage: 0,
            stack_usage: 0,
            total_allocations: 0,
            total_deallocations: 0,
            rss_memory: 0,
        }
    }
}

impl MetricsCollector {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            latencies: Vec::new(),
            throughput_samples: Vec::new(),
            memory_snapshots: Vec::new(),
            cpu_samples: Vec::new(),
            error_count: 0,
            message_count: 0,
            window_size: Duration::from_secs(1),
        }
    }

    /// Create a metrics collector with custom window size.
    pub fn with_window_size(window_size: Duration) -> Self {
        Self {
            window_size,
            ..Self::new()
        }
    }

    /// Record a latency measurement.
    pub fn record_latency(&mut self, latency: Duration) {
        self.latencies.push(latency);
    }

    /// Record message processing.
    pub fn record_message(&mut self) {
        self.message_count += 1;
        
        // Record throughput sample if window elapsed
        let now = Instant::now();
        if self.throughput_samples.is_empty() || 
           now.duration_since(self.throughput_samples.last().unwrap().0) >= self.window_size {
            self.throughput_samples.push((now, self.message_count));
        }
    }

    /// Record batch of messages.
    pub fn record_batch(&mut self, count: usize) {
        self.message_count += count as u64;
        
        let now = Instant::now();
        if self.throughput_samples.is_empty() || 
           now.duration_since(self.throughput_samples.last().unwrap().0) >= self.window_size {
            self.throughput_samples.push((now, self.message_count));
        }
    }

    /// Record an error.
    pub fn record_error(&mut self) {
        self.error_count += 1;
    }

    /// Record memory snapshot.
    pub fn record_memory_snapshot(&mut self, snapshot: MemorySnapshot) {
        self.memory_snapshots.push((Instant::now(), snapshot));
    }

    /// Record CPU usage sample.
    pub fn record_cpu_usage(&mut self, cpu_percent: f64) {
        self.cpu_samples.push((Instant::now(), cpu_percent));
    }

    /// Get current throughput (messages per second).
    pub fn current_throughput(&self) -> u64 {
        if self.throughput_samples.len() < 2 {
            return 0;
        }

        let (last_time, last_count) = self.throughput_samples.last().unwrap();
        let (prev_time, prev_count) = &self.throughput_samples[self.throughput_samples.len() - 2];
        
        let duration = last_time.duration_since(*prev_time);
        let count_diff = last_count.saturating_sub(*prev_count);
        
        if duration.is_zero() {
            0
        } else {
            (count_diff as f64 / duration.as_secs_f64()) as u64
        }
    }

    /// Calculate average throughput over entire collection period.
    pub fn average_throughput(&self) -> u64 {
        let elapsed = self.start_time.elapsed();
        if elapsed.is_zero() {
            0
        } else {
            (self.message_count as f64 / elapsed.as_secs_f64()) as u64
        }
    }

    /// Get latency percentiles.
    pub fn latency_percentiles(&self) -> crate::utils::latency::LatencyStats {
        crate::utils::latency::LatencyStats::from_measurements(self.latencies.clone())
    }

    /// Get memory usage statistics.
    pub fn memory_stats(&self) -> MemoryMetrics {
        if self.memory_snapshots.is_empty() {
            return MemoryMetrics::default();
        }

        let mut peak_memory = 0;
        let mut total_memory = 0;
        let mut total_allocations = 0;
        let mut total_deallocations = 0;

        for (_, snapshot) in &self.memory_snapshots {
            peak_memory = peak_memory.max(snapshot.heap_usage);
            total_memory += snapshot.heap_usage;
            total_allocations = snapshot.total_allocations;
            total_deallocations = snapshot.total_deallocations;
        }

        let average_memory = total_memory / self.memory_snapshots.len() as u64;
        let memory_leaks = total_allocations as i64 - total_deallocations as i64;

        MemoryMetrics {
            peak_memory,
            average_memory,
            total_allocations,
            total_deallocations,
            memory_leaks,
        }
    }

    /// Get average CPU utilization.
    pub fn average_cpu_utilization(&self) -> f64 {
        if self.cpu_samples.is_empty() {
            return 0.0;
        }

        let total: f64 = self.cpu_samples.iter().map(|(_, cpu)| cpu).sum();
        total / self.cpu_samples.len() as f64
    }

    /// Compile comprehensive benchmark metrics.
    pub fn compile_metrics(&self) -> BenchmarkMetrics {
        let duration = self.start_time.elapsed();
        let latency_stats = self.latency_percentiles();
        let memory_usage = self.memory_stats();
        let cpu_utilization = self.average_cpu_utilization();
        let throughput = self.average_throughput();

        BenchmarkMetrics {
            throughput,
            latency_stats,
            memory_usage,
            cpu_utilization,
            duration,
            message_count: self.message_count,
            error_count: self.error_count,
        }
    }

    /// Reset all collected metrics.
    pub fn reset(&mut self) {
        self.start_time = Instant::now();
        self.latencies.clear();
        self.throughput_samples.clear();
        self.memory_snapshots.clear();
        self.cpu_samples.clear();
        self.error_count = 0;
        self.message_count = 0;
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance trends analyzer.
pub struct TrendAnalyzer {
    /// Historical metrics data
    history: Vec<(String, Instant, BenchmarkMetrics)>,
    /// Maximum history size
    max_history: usize,
}

impl TrendAnalyzer {
    /// Create a new trend analyzer.
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            max_history: 1000,
        }
    }

    /// Create a trend analyzer with custom history size.
    pub fn with_max_history(max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            max_history,
        }
    }

    /// Add benchmark result to history.
    pub fn add_result(&mut self, benchmark_name: String, metrics: BenchmarkMetrics) {
        self.history.push((benchmark_name, Instant::now(), metrics));
        
        // Keep only the most recent results
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    /// Get trend for a specific benchmark.
    pub fn get_trend(&self, benchmark_name: &str) -> Vec<&BenchmarkMetrics> {
        self.history
            .iter()
            .filter(|(name, _, _)| name == benchmark_name)
            .map(|(_, _, metrics)| metrics)
            .collect()
    }

    /// Calculate throughput trend (percentage change).
    pub fn throughput_trend(&self, benchmark_name: &str) -> Option<f64> {
        let results = self.get_trend(benchmark_name);
        if results.len() < 2 {
            return None;
        }

        let latest = results.last().unwrap().throughput;
        let previous = results[results.len() - 2].throughput;

        if previous == 0 {
            return None;
        }

        Some(((latest as f64 - previous as f64) / previous as f64) * 100.0)
    }

    /// Calculate latency trend (percentage change in P99).
    pub fn latency_trend(&self, benchmark_name: &str) -> Option<f64> {
        let results = self.get_trend(benchmark_name);
        if results.len() < 2 {
            return None;
        }

        let latest = results.last().unwrap().latency_stats.p99.as_nanos() as f64;
        let previous = results[results.len() - 2].latency_stats.p99.as_nanos() as f64;

        if previous == 0.0 {
            return None;
        }

        Some(((latest - previous) / previous) * 100.0)
    }

    /// Detect performance regressions.
    pub fn detect_regressions(&self, benchmark_name: &str, threshold_percent: f64) -> Vec<RegressionAlert> {
        let mut alerts = Vec::new();

        if let Some(throughput_change) = self.throughput_trend(benchmark_name) {
            if throughput_change < -threshold_percent {
                alerts.push(RegressionAlert {
                    benchmark_name: benchmark_name.to_string(),
                    metric: "throughput".to_string(),
                    change_percent: throughput_change,
                    severity: if throughput_change < -threshold_percent * 2.0 {
                        Severity::Critical
                    } else {
                        Severity::Warning
                    },
                });
            }
        }

        if let Some(latency_change) = self.latency_trend(benchmark_name) {
            if latency_change > threshold_percent {
                alerts.push(RegressionAlert {
                    benchmark_name: benchmark_name.to_string(),
                    metric: "latency_p99".to_string(),
                    change_percent: latency_change,
                    severity: if latency_change > threshold_percent * 2.0 {
                        Severity::Critical
                    } else {
                        Severity::Warning
                    },
                });
            }
        }

        alerts
    }

    /// Generate trend report.
    pub fn generate_report(&self) -> TrendReport {
        let mut benchmark_summaries = HashMap::new();

        // Group by benchmark name
        for (name, _, _) in &self.history {
            benchmark_summaries.entry(name.clone()).or_insert_with(Vec::new);
        }

        for (name, _) in &benchmark_summaries {
            let trend = self.get_trend(name);
            if !trend.is_empty() {
                let latest = trend.last().unwrap();
                let throughput_trend = self.throughput_trend(name);
                let latency_trend = self.latency_trend(name);

                benchmark_summaries.insert(name.clone(), vec![BenchmarkSummary {
                    name: name.clone(),
                    latest_throughput: latest.throughput,
                    latest_p99_latency: latest.latency_stats.p99,
                    throughput_change: throughput_trend,
                    latency_change: latency_trend,
                    run_count: trend.len(),
                }]);
            }
        }

        TrendReport {
            generated_at: Instant::now(),
            benchmarks: benchmark_summaries.into_values().flatten().collect(),
        }
    }
}

impl Default for TrendAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance regression alert.
#[derive(Debug, Clone)]
pub struct RegressionAlert {
    /// Benchmark name
    pub benchmark_name: String,
    /// Metric that regressed
    pub metric: String,
    /// Percentage change
    pub change_percent: f64,
    /// Alert severity
    pub severity: Severity,
}

/// Alert severity levels.
#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    /// Information only
    Info,
    /// Warning level
    Warning,
    /// Critical regression
    Critical,
}

/// Benchmark summary for reports.
#[derive(Debug, Clone)]
pub struct BenchmarkSummary {
    /// Benchmark name
    pub name: String,
    /// Latest throughput measurement
    pub latest_throughput: u64,
    /// Latest P99 latency measurement
    pub latest_p99_latency: Duration,
    /// Throughput trend (percentage change)
    pub throughput_change: Option<f64>,
    /// Latency trend (percentage change)
    pub latency_change: Option<f64>,
    /// Number of runs in history
    pub run_count: usize,
}

/// Trend analysis report.
#[derive(Debug, Clone)]
pub struct TrendReport {
    /// When the report was generated
    pub generated_at: Instant,
    /// Summary of all benchmarks
    pub benchmarks: Vec<BenchmarkSummary>,
}

impl TrendReport {
    /// Print formatted report to stdout.
    pub fn print(&self) {
        println!("\n=== Performance Trend Report ===");
        println!("Generated at: {:?}", self.generated_at);
        println!("Total benchmarks: {}", self.benchmarks.len());
        println!();

        for summary in &self.benchmarks {
            println!("Benchmark: {}", summary.name);
            println!("  Latest throughput: {} msg/sec", summary.latest_throughput);
            println!("  Latest P99 latency: {:?}", summary.latest_p99_latency);
            
            if let Some(change) = summary.throughput_change {
                let indicator = if change >= 0.0 { "↗" } else { "↘" };
                println!("  Throughput trend: {:.2}% {}", change, indicator);
            } else {
                println!("  Throughput trend: N/A");
            }
            
            if let Some(change) = summary.latency_change {
                let indicator = if change >= 0.0 { "↗" } else { "↘" };
                println!("  Latency trend: {:.2}% {}", change, indicator);
            } else {
                println!("  Latency trend: N/A");
            }
            
            println!("  Run count: {}", summary.run_count);
            println!();
        }
        
        println!("================================\n");
    }

    /// Export report as JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        #[derive(serde::Serialize)]
        struct JsonReport {
            generated_at: u64,
            benchmarks: Vec<JsonBenchmark>,
        }

        #[derive(serde::Serialize)]
        struct JsonBenchmark {
            name: String,
            latest_throughput: u64,
            latest_p99_latency_nanos: u64,
            throughput_change: Option<f64>,
            latency_change: Option<f64>,
            run_count: usize,
        }

        let json_report = JsonReport {
            generated_at: self.generated_at.elapsed().as_nanos() as u64,
            benchmarks: self.benchmarks.iter().map(|b| JsonBenchmark {
                name: b.name.clone(),
                latest_throughput: b.latest_throughput,
                latest_p99_latency_nanos: b.latest_p99_latency.as_nanos() as u64,
                throughput_change: b.throughput_change,
                latency_change: b.latency_change,
                run_count: b.run_count,
            }).collect(),
        };

        serde_json::to_string_pretty(&json_report)
    }
}

/// Metrics export utilities.
pub mod export {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;

    /// Export metrics to CSV format.
    pub fn to_csv(metrics: &[BenchmarkMetrics], path: &Path) -> Result<(), std::io::Error> {
        let mut file = File::create(path)?;
        
        // Write header
        writeln!(file, "throughput,p50_latency_ns,p95_latency_ns,p99_latency_ns,p999_latency_ns,duration_ms,message_count,error_count,cpu_utilization,peak_memory")?;
        
        // Write data
        for metric in metrics {
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{},{}",
                metric.throughput,
                metric.latency_stats.p50.as_nanos(),
                metric.latency_stats.p95.as_nanos(),
                metric.latency_stats.p99.as_nanos(),
                metric.latency_stats.p999.as_nanos(),
                metric.duration.as_millis(),
                metric.message_count,
                metric.error_count,
                metric.cpu_utilization,
                metric.memory_usage.peak_memory
            )?;
        }
        
        Ok(())
    }

    /// Export metrics to JSON format.
    pub fn to_json(metrics: &[BenchmarkMetrics], path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, metrics)?;
        Ok(())
    }
}