//! Performance regression detection and tracking.

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use crate::{BenchmarkMetrics, validators::{RegressionAlert, RegressionSeverity}};

/// Persistent storage for benchmark baselines and history.
pub struct RegressionTracker {
    /// Storage directory for baseline data
    storage_dir: PathBuf,
    /// Regression detection threshold (percentage)
    threshold: f64,
    /// Maximum number of historical entries to keep
    max_history: usize,
    /// Current baselines by benchmark name
    baselines: HashMap<String, BaselineEntry>,
    /// Performance history
    history: HashMap<String, Vec<HistoryEntry>>,
}

/// Baseline entry for regression detection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BaselineEntry {
    /// Benchmark name
    pub benchmark_name: String,
    /// Baseline metrics
    pub metrics: BenchmarkMetrics,
    /// When this baseline was established
    pub timestamp: u64,
    /// Git commit hash when baseline was set
    pub commit_hash: Option<String>,
    /// Environment information
    pub environment: EnvironmentInfo,
}

/// Historical performance entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    /// Performance metrics
    pub metrics: BenchmarkMetrics,
    /// Timestamp of the run
    pub timestamp: u64,
    /// Git commit hash
    pub commit_hash: Option<String>,
    /// Any detected regressions
    pub regressions: Vec<RegressionData>,
}

/// Serializable regression data.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegressionData {
    /// Metric name
    pub metric: String,
    /// Baseline value
    pub baseline_value: String,
    /// Current value
    pub current_value: String,
    /// Percentage change
    pub change_percent: f64,
    /// Severity level
    pub severity: String,
}

/// Environment information for reproducibility.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnvironmentInfo {
    /// Operating system
    pub os: String,
    /// CPU information
    pub cpu_info: String,
    /// Memory size
    pub memory_size: u64,
    /// Rust version
    pub rust_version: String,
    /// Compiler flags
    pub compiler_flags: Option<String>,
}

impl Default for EnvironmentInfo {
    fn default() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            cpu_info: format!("{} cores", num_cpus::get()),
            memory_size: 0, // Would need system info crate for actual memory
            rust_version: env!("RUSTC_VERSION").to_string(),
            compiler_flags: None,
        }
    }
}

impl RegressionTracker {
    /// Create a new regression tracker.
    pub fn new<P: AsRef<Path>>(storage_dir: P, threshold: f64) -> Result<Self, Box<dyn std::error::Error>> {
        let storage_dir = storage_dir.as_ref().to_path_buf();
        
        // Create storage directory if it doesn't exist
        if !storage_dir.exists() {
            fs::create_dir_all(&storage_dir)?;
        }

        let mut tracker = Self {
            storage_dir,
            threshold,
            max_history: 100,
            baselines: HashMap::new(),
            history: HashMap::new(),
        };

        // Load existing data
        tracker.load_data()?;

        Ok(tracker)
    }

    /// Set maximum number of historical entries to keep.
    pub fn with_max_history(mut self, max_history: usize) -> Self {
        self.max_history = max_history;
        self
    }

    /// Set baseline for a benchmark.
    pub fn set_baseline(&mut self, benchmark_name: String, metrics: BenchmarkMetrics) -> Result<(), Box<dyn std::error::Error>> {
        let baseline = BaselineEntry {
            benchmark_name: benchmark_name.clone(),
            metrics,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            commit_hash: self.get_git_commit_hash(),
            environment: EnvironmentInfo::default(),
        };

        self.baselines.insert(benchmark_name, baseline);
        self.save_baselines()?;
        
        Ok(())
    }

    /// Add performance result and detect regressions.
    pub fn add_result(&mut self, benchmark_name: String, metrics: BenchmarkMetrics) -> Result<Vec<RegressionAlert>, Box<dyn std::error::Error>> {
        let mut alerts = Vec::new();

        // Detect regressions if baseline exists
        if let Some(baseline) = self.baselines.get(&benchmark_name) {
            alerts = self.detect_regressions(&baseline.metrics, &metrics);
        }

        // Convert alerts to regression data
        let regression_data: Vec<RegressionData> = alerts.iter().map(|alert| {
            RegressionData {
                metric: alert.metric.clone(),
                baseline_value: alert.baseline_value.clone(),
                current_value: alert.current_value.clone(),
                change_percent: alert.change_percent,
                severity: format!("{:?}", alert.severity),
            }
        }).collect();

        // Add to history
        let entry = HistoryEntry {
            metrics,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            commit_hash: self.get_git_commit_hash(),
            regressions: regression_data,
        };

        let history = self.history.entry(benchmark_name).or_insert_with(Vec::new);
        history.push(entry);

        // Trim history to max size
        if history.len() > self.max_history {
            history.drain(0..history.len() - self.max_history);
        }

        self.save_history()?;

        Ok(alerts)
    }

    /// Detect regressions between baseline and current metrics.
    fn detect_regressions(&self, baseline: &BenchmarkMetrics, current: &BenchmarkMetrics) -> Vec<RegressionAlert> {
        let mut alerts = Vec::new();

        // Throughput regression
        let throughput_change = self.calculate_percentage_change(
            baseline.throughput as f64,
            current.throughput as f64,
        );

        if throughput_change < -self.threshold {
            alerts.push(RegressionAlert {
                metric: "throughput".to_string(),
                baseline_value: format!("{} msg/sec", baseline.throughput),
                current_value: format!("{} msg/sec", current.throughput),
                change_percent: throughput_change,
                severity: self.determine_severity(throughput_change.abs()),
            });
        }

        // P99 latency regression
        let p99_change = self.calculate_percentage_change(
            baseline.latency_stats.p99.as_nanos() as f64,
            current.latency_stats.p99.as_nanos() as f64,
        );

        if p99_change > self.threshold {
            alerts.push(RegressionAlert {
                metric: "p99_latency".to_string(),
                baseline_value: format!("{:?}", baseline.latency_stats.p99),
                current_value: format!("{:?}", current.latency_stats.p99),
                change_percent: p99_change,
                severity: self.determine_severity(p99_change),
            });
        }

        // Mean latency regression
        let mean_change = self.calculate_percentage_change(
            baseline.latency_stats.mean.as_nanos() as f64,
            current.latency_stats.mean.as_nanos() as f64,
        );

        if mean_change > self.threshold {
            alerts.push(RegressionAlert {
                metric: "mean_latency".to_string(),
                baseline_value: format!("{:?}", baseline.latency_stats.mean),
                current_value: format!("{:?}", current.latency_stats.mean),
                change_percent: mean_change,
                severity: self.determine_severity(mean_change),
            });
        }

        // Memory usage regression
        let memory_change = self.calculate_percentage_change(
            baseline.memory_usage.peak_memory as f64,
            current.memory_usage.peak_memory as f64,
        );

        if memory_change > self.threshold * 2.0 { // More lenient for memory
            alerts.push(RegressionAlert {
                metric: "peak_memory".to_string(),
                baseline_value: format!("{} bytes", baseline.memory_usage.peak_memory),
                current_value: format!("{} bytes", current.memory_usage.peak_memory),
                change_percent: memory_change,
                severity: self.determine_severity(memory_change),
            });
        }

        // Error rate regression
        let baseline_error_rate = if baseline.message_count > 0 {
            (baseline.error_count as f64 / baseline.message_count as f64) * 100.0
        } else {
            0.0
        };

        let current_error_rate = if current.message_count > 0 {
            (current.error_count as f64 / current.message_count as f64) * 100.0
        } else {
            0.0
        };

        let error_rate_change = self.calculate_percentage_change(baseline_error_rate, current_error_rate);

        if error_rate_change > self.threshold && current_error_rate > 0.1 { // Only alert if error rate > 0.1%
            alerts.push(RegressionAlert {
                metric: "error_rate".to_string(),
                baseline_value: format!("{:.3}%", baseline_error_rate),
                current_value: format!("{:.3}%", current_error_rate),
                change_percent: error_rate_change,
                severity: self.determine_severity(error_rate_change),
            });
        }

        alerts
    }

    /// Calculate percentage change between two values.
    fn calculate_percentage_change(&self, baseline: f64, current: f64) -> f64 {
        if baseline == 0.0 {
            if current > 0.0 {
                100.0 // 100% increase from zero
            } else {
                0.0
            }
        } else {
            ((current - baseline) / baseline) * 100.0
        }
    }

    /// Determine severity based on change magnitude.
    fn determine_severity(&self, change_percent: f64) -> RegressionSeverity {
        if change_percent >= self.threshold * 3.0 {
            RegressionSeverity::Critical
        } else if change_percent >= self.threshold * 2.0 {
            RegressionSeverity::Major
        } else {
            RegressionSeverity::Minor
        }
    }

    /// Get performance trend for a benchmark.
    pub fn get_trend(&self, benchmark_name: &str, metric: &str) -> Vec<(u64, f64)> {
        let Some(history) = self.history.get(benchmark_name) else {
            return Vec::new();
        };

        history.iter().map(|entry| {
            let value = match metric {
                "throughput" => entry.metrics.throughput as f64,
                "p99_latency" => entry.metrics.latency_stats.p99.as_nanos() as f64,
                "mean_latency" => entry.metrics.latency_stats.mean.as_nanos() as f64,
                "peak_memory" => entry.metrics.memory_usage.peak_memory as f64,
                "cpu_utilization" => entry.metrics.cpu_utilization,
                _ => 0.0,
            };
            (entry.timestamp, value)
        }).collect()
    }

    /// Generate regression report for all benchmarks.
    pub fn generate_report(&self) -> RegressionReport {
        let mut benchmark_reports = Vec::new();

        for (name, history) in &self.history {
            if let Some(latest) = history.last() {
                let has_regressions = !latest.regressions.is_empty();
                let regression_count = latest.regressions.len();
                
                benchmark_reports.push(BenchmarkRegressionReport {
                    benchmark_name: name.clone(),
                    latest_timestamp: latest.timestamp,
                    has_regressions,
                    regression_count,
                    regressions: latest.regressions.clone(),
                });
            }
        }

        RegressionReport {
            generated_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            total_benchmarks: benchmark_reports.len(),
            benchmarks_with_regressions: benchmark_reports.iter().filter(|r| r.has_regressions).count(),
            benchmark_reports,
        }
    }

    /// Get git commit hash if available.
    fn get_git_commit_hash(&self) -> Option<String> {
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }

    /// Load existing baseline and history data.
    fn load_data(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Load baselines
        let baselines_path = self.storage_dir.join("baselines.json");
        if baselines_path.exists() {
            let data = fs::read_to_string(&baselines_path)?;
            self.baselines = serde_json::from_str(&data)?;
        }

        // Load history
        let history_path = self.storage_dir.join("history.json");
        if history_path.exists() {
            let data = fs::read_to_string(&history_path)?;
            self.history = serde_json::from_str(&data)?;
        }

        Ok(())
    }

    /// Save baselines to persistent storage.
    fn save_baselines(&self) -> Result<(), Box<dyn std::error::Error>> {
        let baselines_path = self.storage_dir.join("baselines.json");
        let data = serde_json::to_string_pretty(&self.baselines)?;
        fs::write(baselines_path, data)?;
        Ok(())
    }

    /// Save history to persistent storage.
    fn save_history(&self) -> Result<(), Box<dyn std::error::Error>> {
        let history_path = self.storage_dir.join("history.json");
        let data = serde_json::to_string_pretty(&self.history)?;
        fs::write(history_path, data)?;
        Ok(())
    }
}

/// Regression report for all benchmarks.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegressionReport {
    /// When the report was generated
    pub generated_at: u64,
    /// Total number of benchmarks
    pub total_benchmarks: usize,
    /// Number of benchmarks with regressions
    pub benchmarks_with_regressions: usize,
    /// Per-benchmark regression reports
    pub benchmark_reports: Vec<BenchmarkRegressionReport>,
}

/// Regression report for a single benchmark.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkRegressionReport {
    /// Benchmark name
    pub benchmark_name: String,
    /// Timestamp of latest run
    pub latest_timestamp: u64,
    /// Whether this benchmark has regressions
    pub has_regressions: bool,
    /// Number of regressions detected
    pub regression_count: usize,
    /// Detailed regression information
    pub regressions: Vec<RegressionData>,
}

impl RegressionReport {
    /// Print formatted report.
    pub fn print(&self) {
        println!("\n=== Regression Detection Report ===");
        println!("Generated at: {}", self.generated_at);
        println!("Total benchmarks: {}", self.total_benchmarks);
        println!("Benchmarks with regressions: {}", self.benchmarks_with_regressions);
        
        if self.benchmarks_with_regressions > 0 {
            let rate = (self.benchmarks_with_regressions as f64 / self.total_benchmarks as f64) * 100.0;
            println!("Regression rate: {:.1}%", rate);
        }
        println!();

        // Print regressions by benchmark
        for report in &self.benchmark_reports {
            if report.has_regressions {
                println!("🔴 {}: {} regressions", report.benchmark_name, report.regression_count);
                for regression in &report.regressions {
                    println!("  - {}: {:.2}% change ({})", 
                        regression.metric, 
                        regression.change_percent,
                        regression.severity
                    );
                    println!("    {} → {}", regression.baseline_value, regression.current_value);
                }
                println!();
            }
        }

        // Print clean benchmarks
        let clean_benchmarks: Vec<_> = self.benchmark_reports.iter()
            .filter(|r| !r.has_regressions)
            .collect();
        
        if !clean_benchmarks.is_empty() {
            println!("✅ Clean benchmarks ({}):", clean_benchmarks.len());
            for report in clean_benchmarks {
                println!("  - {}", report.benchmark_name);
            }
        }
        
        println!("===================================\n");
    }

    /// Save report to JSON file.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let data = serde_json::to_string_pretty(self)?;
        fs::write(path, data)?;
        Ok(())
    }
}

/// Utilities for CI/CD integration.
pub mod ci {
    use super::*;

    /// Exit codes for CI integration.
    pub enum ExitCode {
        /// No regressions detected
        Success = 0,
        /// Minor regressions detected
        MinorRegression = 1,
        /// Major regressions detected
        MajorRegression = 2,
        /// Critical regressions detected
        CriticalRegression = 3,
    }

    /// Determine appropriate exit code based on regressions.
    pub fn determine_exit_code(alerts: &[RegressionAlert]) -> ExitCode {
        if alerts.is_empty() {
            return ExitCode::Success;
        }

        let max_severity = alerts.iter()
            .map(|alert| &alert.severity)
            .max()
            .unwrap();

        match max_severity {
            RegressionSeverity::Minor => ExitCode::MinorRegression,
            RegressionSeverity::Major => ExitCode::MajorRegression,
            RegressionSeverity::Critical => ExitCode::CriticalRegression,
        }
    }

    /// Generate GitHub Actions annotations for regressions.
    pub fn generate_github_annotations(alerts: &[RegressionAlert]) -> Vec<String> {
        alerts.iter().map(|alert| {
            let level = match alert.severity {
                RegressionSeverity::Minor => "warning",
                RegressionSeverity::Major | RegressionSeverity::Critical => "error",
            };

            format!(
                "::{} title=Performance Regression::Metric '{}' regressed by {:.2}%. {} → {}",
                level,
                alert.metric,
                alert.change_percent.abs(),
                alert.baseline_value,
                alert.current_value
            )
        }).collect()
    }
}