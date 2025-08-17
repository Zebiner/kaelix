//! Performance validation and regression detection.

use std::time::Duration;
use crate::{BenchmarkConfig, BenchmarkMetrics, MemoryMetrics};

/// Performance validation results.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the validation passed
    pub passed: bool,
    /// Validation message
    pub message: String,
    /// Actual value measured
    pub actual_value: String,
    /// Expected value or threshold
    pub expected_value: String,
    /// Validation category
    pub category: ValidationCategory,
}

/// Categories of performance validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationCategory {
    /// Throughput validation
    Throughput,
    /// Latency validation
    Latency,
    /// Memory usage validation
    Memory,
    /// CPU utilization validation
    Cpu,
    /// Error rate validation
    ErrorRate,
    /// General performance
    Performance,
}

impl ValidationResult {
    /// Create a passing validation result.
    pub fn pass(category: ValidationCategory, message: String, actual: String, expected: String) -> Self {
        Self {
            passed: true,
            message,
            actual_value: actual,
            expected_value: expected,
            category,
        }
    }

    /// Create a failing validation result.
    pub fn fail(category: ValidationCategory, message: String, actual: String, expected: String) -> Self {
        Self {
            passed: false,
            message,
            actual_value: actual,
            expected_value: expected,
            category,
        }
    }
}

/// Comprehensive performance validator.
pub struct PerformanceValidator {
    /// Configuration for validation thresholds
    config: BenchmarkConfig,
}

impl PerformanceValidator {
    /// Create a new performance validator.
    pub fn new(config: BenchmarkConfig) -> Self {
        Self { config }
    }

    /// Validate all performance metrics.
    pub fn validate_all(&self, metrics: &BenchmarkMetrics) -> Vec<ValidationResult> {
        let mut results = Vec::new();

        results.extend(self.validate_throughput(metrics.throughput));
        results.extend(self.validate_latency_stats(&metrics.latency_stats));
        results.extend(self.validate_memory_usage(&metrics.memory_usage));
        results.extend(self.validate_cpu_utilization(metrics.cpu_utilization));
        results.extend(self.validate_error_rate(metrics));

        results
    }

    /// Validate throughput against targets.
    pub fn validate_throughput(&self, throughput: u64) -> Vec<ValidationResult> {
        let mut results = Vec::new();

        let target = self.config.target_throughput;
        if throughput >= target {
            results.push(ValidationResult::pass(
                ValidationCategory::Throughput,
                "Throughput meets target".to_string(),
                format!("{} msg/sec", throughput),
                format!(">= {} msg/sec", target),
            ));
        } else {
            results.push(ValidationResult::fail(
                ValidationCategory::Throughput,
                "Throughput below target".to_string(),
                format!("{} msg/sec", throughput),
                format!(">= {} msg/sec", target),
            ));
        }

        // Additional throughput validations
        let min_acceptable = target / 2; // 50% of target is minimum acceptable
        if throughput < min_acceptable {
            results.push(ValidationResult::fail(
                ValidationCategory::Throughput,
                "Throughput critically low".to_string(),
                format!("{} msg/sec", throughput),
                format!(">= {} msg/sec", min_acceptable),
            ));
        }

        results
    }

    /// Validate latency statistics.
    pub fn validate_latency_stats(&self, stats: &crate::utils::latency::LatencyStats) -> Vec<ValidationResult> {
        let mut results = Vec::new();

        // Validate P99 latency
        let target_p99 = self.config.max_latency;
        if stats.p99 <= target_p99 {
            results.push(ValidationResult::pass(
                ValidationCategory::Latency,
                "P99 latency meets target".to_string(),
                format!("{:?}", stats.p99),
                format!("<= {:?}", target_p99),
            ));
        } else {
            results.push(ValidationResult::fail(
                ValidationCategory::Latency,
                "P99 latency exceeds target".to_string(),
                format!("{:?}", stats.p99),
                format!("<= {:?}", target_p99),
            ));
        }

        // Validate P95 latency (should be better than P99)
        let target_p95 = target_p99 * 80 / 100; // 80% of P99 target
        if stats.p95 <= target_p95 {
            results.push(ValidationResult::pass(
                ValidationCategory::Latency,
                "P95 latency acceptable".to_string(),
                format!("{:?}", stats.p95),
                format!("<= {:?}", target_p95),
            ));
        } else {
            results.push(ValidationResult::fail(
                ValidationCategory::Latency,
                "P95 latency too high".to_string(),
                format!("{:?}", stats.p95),
                format!("<= {:?}", target_p95),
            ));
        }

        // Validate mean latency
        let target_mean = target_p99 / 2; // Mean should be much better than P99
        if stats.mean <= target_mean {
            results.push(ValidationResult::pass(
                ValidationCategory::Latency,
                "Mean latency acceptable".to_string(),
                format!("{:?}", stats.mean),
                format!("<= {:?}", target_mean),
            ));
        } else {
            results.push(ValidationResult::fail(
                ValidationCategory::Latency,
                "Mean latency too high".to_string(),
                format!("{:?}", stats.mean),
                format!("<= {:?}", target_mean),
            ));
        }

        results
    }

    /// Validate memory usage.
    pub fn validate_memory_usage(&self, memory: &MemoryMetrics) -> Vec<ValidationResult> {
        let mut results = Vec::new();

        // Check for memory leaks
        if memory.memory_leaks <= 0 {
            results.push(ValidationResult::pass(
                ValidationCategory::Memory,
                "No memory leaks detected".to_string(),
                format!("{} bytes", memory.memory_leaks),
                "<= 0 bytes".to_string(),
            ));
        } else {
            let severity = if memory.memory_leaks > 1024 * 1024 { "Critical" } else { "Warning" };
            results.push(ValidationResult::fail(
                ValidationCategory::Memory,
                format!("{} memory leak detected", severity),
                format!("{} bytes", memory.memory_leaks),
                "<= 0 bytes".to_string(),
            ));
        }

        // Validate reasonable memory usage
        let max_acceptable_memory = 2 * 1024 * 1024 * 1024; // 2GB
        if memory.peak_memory <= max_acceptable_memory {
            results.push(ValidationResult::pass(
                ValidationCategory::Memory,
                "Peak memory usage acceptable".to_string(),
                format!("{} bytes", memory.peak_memory),
                format!("<= {} bytes", max_acceptable_memory),
            ));
        } else {
            results.push(ValidationResult::fail(
                ValidationCategory::Memory,
                "Peak memory usage too high".to_string(),
                format!("{} bytes", memory.peak_memory),
                format!("<= {} bytes", max_acceptable_memory),
            ));
        }

        results
    }

    /// Validate CPU utilization.
    pub fn validate_cpu_utilization(&self, cpu_percent: f64) -> Vec<ValidationResult> {
        let mut results = Vec::new();

        // CPU utilization should be reasonable (not too high, not too low)
        let min_cpu = 10.0; // At least 10% CPU usage under load
        let max_cpu = 95.0; // No more than 95% CPU usage

        if cpu_percent >= min_cpu && cpu_percent <= max_cpu {
            results.push(ValidationResult::pass(
                ValidationCategory::Cpu,
                "CPU utilization within acceptable range".to_string(),
                format!("{:.2}%", cpu_percent),
                format!("{}% - {}%", min_cpu, max_cpu),
            ));
        } else if cpu_percent < min_cpu {
            results.push(ValidationResult::fail(
                ValidationCategory::Cpu,
                "CPU utilization too low - possible performance issue".to_string(),
                format!("{:.2}%", cpu_percent),
                format!(">= {:.2}%", min_cpu),
            ));
        } else {
            results.push(ValidationResult::fail(
                ValidationCategory::Cpu,
                "CPU utilization too high - possible inefficiency".to_string(),
                format!("{:.2}%", cpu_percent),
                format!("<= {:.2}%", max_cpu),
            ));
        }

        results
    }

    /// Validate error rate.
    pub fn validate_error_rate(&self, metrics: &BenchmarkMetrics) -> Vec<ValidationResult> {
        let mut results = Vec::new();

        let error_rate = if metrics.message_count > 0 {
            (metrics.error_count as f64 / metrics.message_count as f64) * 100.0
        } else {
            0.0
        };

        let max_error_rate = 1.0; // Maximum 1% error rate

        if error_rate <= max_error_rate {
            results.push(ValidationResult::pass(
                ValidationCategory::ErrorRate,
                "Error rate within acceptable limits".to_string(),
                format!("{:.3}%", error_rate),
                format!("<= {:.1}%", max_error_rate),
            ));
        } else {
            results.push(ValidationResult::fail(
                ValidationCategory::ErrorRate,
                "Error rate too high".to_string(),
                format!("{:.3}%", error_rate),
                format!("<= {:.1}%", max_error_rate),
            ));
        }

        results
    }

    /// Generate validation summary.
    pub fn validation_summary(&self, results: &[ValidationResult]) -> ValidationSummary {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;

        let by_category = results.iter().fold(
            std::collections::HashMap::new(),
            |mut acc, result| {
                let entry = acc.entry(result.category.clone()).or_insert((0, 0));
                if result.passed {
                    entry.0 += 1;
                } else {
                    entry.1 += 1;
                }
                acc
            }
        );

        ValidationSummary {
            total_tests: total,
            passed_tests: passed,
            failed_tests: failed,
            success_rate: if total > 0 { (passed as f64 / total as f64) * 100.0 } else { 0.0 },
            by_category,
            overall_status: if failed == 0 { ValidationStatus::Pass } else { ValidationStatus::Fail },
        }
    }
}

/// Summary of validation results.
#[derive(Debug, Clone)]
pub struct ValidationSummary {
    /// Total number of validation tests
    pub total_tests: usize,
    /// Number of tests that passed
    pub passed_tests: usize,
    /// Number of tests that failed
    pub failed_tests: usize,
    /// Success rate as percentage
    pub success_rate: f64,
    /// Results broken down by category (passed, failed)
    pub by_category: std::collections::HashMap<ValidationCategory, (usize, usize)>,
    /// Overall validation status
    pub overall_status: ValidationStatus,
}

/// Overall validation status.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationStatus {
    /// All validations passed
    Pass,
    /// Some validations failed
    Fail,
}

impl ValidationSummary {
    /// Print detailed validation summary.
    pub fn print_detailed(&self, results: &[ValidationResult]) {
        println!("\n=== Performance Validation Summary ===");
        println!("Total tests: {}", self.total_tests);
        println!("Passed: {}", self.passed_tests);
        println!("Failed: {}", self.failed_tests);
        println!("Success rate: {:.1}%", self.success_rate);
        println!("Overall status: {:?}", self.overall_status);
        println!();

        // Print by category
        for (category, (passed, failed)) in &self.by_category {
            println!("{:?}: {} passed, {} failed", category, passed, failed);
        }
        println!();

        // Print failed tests details
        let failed_results: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        if !failed_results.is_empty() {
            println!("Failed Validations:");
            for result in failed_results {
                println!("  ❌ {}: {}", result.category, result.message);
                println!("     Expected: {}, Actual: {}", result.expected_value, result.actual_value);
            }
            println!();
        }

        // Print passed tests summary
        let passed_results: Vec<_> = results.iter().filter(|r| r.passed).collect();
        if !passed_results.is_empty() {
            println!("Passed Validations ({}):", passed_results.len());
            for result in passed_results {
                println!("  ✅ {}: {}", result.category, result.message);
            }
        }

        println!("======================================\n");
    }

    /// Export summary as JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        #[derive(serde::Serialize)]
        struct JsonSummary {
            total_tests: usize,
            passed_tests: usize,
            failed_tests: usize,
            success_rate: f64,
            overall_status: String,
            categories: std::collections::HashMap<String, JsonCategoryResult>,
        }

        #[derive(serde::Serialize)]
        struct JsonCategoryResult {
            passed: usize,
            failed: usize,
        }

        let categories = self.by_category.iter().map(|(cat, (passed, failed))| {
            (format!("{:?}", cat), JsonCategoryResult { passed: *passed, failed: *failed })
        }).collect();

        let json_summary = JsonSummary {
            total_tests: self.total_tests,
            passed_tests: self.passed_tests,
            failed_tests: self.failed_tests,
            success_rate: self.success_rate,
            overall_status: format!("{:?}", self.overall_status),
            categories,
        };

        serde_json::to_string_pretty(&json_summary)
    }
}

/// Regression detection for performance metrics.
pub struct RegressionDetector {
    /// Threshold for regression detection (percentage)
    threshold: f64,
    /// Historical baseline metrics
    baseline: Option<BenchmarkMetrics>,
}

impl RegressionDetector {
    /// Create a new regression detector.
    pub fn new(threshold_percent: f64) -> Self {
        Self {
            threshold: threshold_percent,
            baseline: None,
        }
    }

    /// Set baseline metrics for regression detection.
    pub fn set_baseline(&mut self, baseline: BenchmarkMetrics) {
        self.baseline = Some(baseline);
    }

    /// Detect regressions in current metrics against baseline.
    pub fn detect_regressions(&self, current: &BenchmarkMetrics) -> Vec<RegressionAlert> {
        let Some(ref baseline) = self.baseline else {
            return Vec::new();
        };

        let mut alerts = Vec::new();

        // Check throughput regression
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

        // Check P99 latency regression
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

        // Check memory usage regression
        let memory_change = self.calculate_percentage_change(
            baseline.memory_usage.peak_memory as f64,
            current.memory_usage.peak_memory as f64,
        );

        if memory_change > self.threshold * 2.0 { // More lenient threshold for memory
            alerts.push(RegressionAlert {
                metric: "peak_memory".to_string(),
                baseline_value: format!("{} bytes", baseline.memory_usage.peak_memory),
                current_value: format!("{} bytes", current.memory_usage.peak_memory),
                change_percent: memory_change,
                severity: self.determine_severity(memory_change),
            });
        }

        alerts
    }

    /// Calculate percentage change between two values.
    fn calculate_percentage_change(&self, baseline: f64, current: f64) -> f64 {
        if baseline == 0.0 {
            return 0.0;
        }
        ((current - baseline) / baseline) * 100.0
    }

    /// Determine severity of regression based on magnitude.
    fn determine_severity(&self, change_percent: f64) -> RegressionSeverity {
        if change_percent >= self.threshold * 3.0 {
            RegressionSeverity::Critical
        } else if change_percent >= self.threshold * 2.0 {
            RegressionSeverity::Major
        } else {
            RegressionSeverity::Minor
        }
    }
}

/// Regression alert information.
#[derive(Debug, Clone)]
pub struct RegressionAlert {
    /// Name of the metric that regressed
    pub metric: String,
    /// Baseline value
    pub baseline_value: String,
    /// Current value
    pub current_value: String,
    /// Percentage change
    pub change_percent: f64,
    /// Severity of the regression
    pub severity: RegressionSeverity,
}

/// Severity levels for regressions.
#[derive(Debug, Clone, PartialEq)]
pub enum RegressionSeverity {
    /// Minor regression
    Minor,
    /// Major regression
    Major,
    /// Critical regression
    Critical,
}

impl RegressionAlert {
    /// Print formatted alert.
    pub fn print(&self) {
        let icon = match self.severity {
            RegressionSeverity::Minor => "⚠️",
            RegressionSeverity::Major => "🔴",
            RegressionSeverity::Critical => "💥",
        };

        println!("{} {:?} Regression in {}", icon, self.severity, self.metric);
        println!("  Baseline: {}", self.baseline_value);
        println!("  Current:  {}", self.current_value);
        println!("  Change:   {:.2}%", self.change_percent);
    }
}