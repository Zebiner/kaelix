//! # Test Reporting System
//!
//! Comprehensive test reporting with support for multiple output formats (HTML, JSON, JUnit XML)
//! and integration with CI/CD systems for automated test result publication.

// Submodules
pub mod dashboard;
pub mod ci;

use crate::metrics::{MetricsSnapshot, TestMetrics};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Status of a test execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    /// Test passed successfully
    Passed,
    /// Test failed with an error
    Failed,
    /// Test was skipped
    Skipped,
    /// Test timed out
    Timeout,
    /// Test is currently running
    Running,
}

impl fmt::Display for TestStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestStatus::Passed => write!(f, "PASSED"),
            TestStatus::Failed => write!(f, "FAILED"),
            TestStatus::Skipped => write!(f, "SKIPPED"),
            TestStatus::Timeout => write!(f, "TIMEOUT"),
            TestStatus::Running => write!(f, "RUNNING"),
        }
    }
}

/// Coverage data for test execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageData {
    /// Lines covered
    pub lines_covered: u64,
    /// Total lines
    pub total_lines: u64,
    /// Functions covered
    pub functions_covered: u64,
    /// Total functions
    pub total_functions: u64,
    /// Branches covered
    pub branches_covered: u64,
    /// Total branches
    pub total_branches: u64,
    /// Coverage percentage
    pub coverage_percent: f64,
}

impl CoverageData {
    /// Calculates overall coverage percentage
    pub fn calculate_coverage(&mut self) {
        if self.total_lines > 0 {
            self.coverage_percent = (self.lines_covered as f64 / self.total_lines as f64) * 100.0;
        }
    }
}

/// Performance summary for test execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    /// Average throughput in messages per second
    pub avg_throughput: f64,
    /// Peak throughput in messages per second
    pub peak_throughput: f64,
    /// Average latency in nanoseconds
    pub avg_latency_ns: u64,
    /// P99 latency in nanoseconds
    pub p99_latency_ns: u64,
    /// Total messages processed
    pub total_messages: u64,
    /// Total errors encountered
    pub total_errors: u64,
    /// Error rate as percentage
    pub error_rate_percent: f64,
    /// Resource utilization summary
    pub resource_summary: ResourceSummary,
}

/// Resource utilization summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSummary {
    /// Peak memory usage in bytes
    pub peak_memory_bytes: u64,
    /// Average CPU usage percentage
    pub avg_cpu_percent: f64,
    /// Peak CPU usage percentage
    pub peak_cpu_percent: f64,
    /// Total network I/O in bytes
    pub total_network_io_bytes: u64,
    /// Total disk I/O in bytes
    pub total_disk_io_bytes: u64,
}

/// Individual test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test name
    pub name: String,
    /// Test status
    pub status: TestStatus,
    /// Test execution duration
    pub duration: Duration,
    /// Error message if test failed
    pub error: Option<String>,
    /// Test-specific metrics
    pub metrics: Option<MetricsSnapshot>,
    /// Test start time
    pub start_time: DateTime<Utc>,
    /// Test end time
    pub end_time: Option<DateTime<Utc>>,
    /// Test tags/labels
    pub tags: HashMap<String, String>,
    /// Test output/logs
    pub output: Option<String>,
}

impl TestResult {
    /// Creates a new test result
    pub fn new(name: String) -> Self {
        Self {
            name,
            status: TestStatus::Running,
            duration: Duration::ZERO,
            error: None,
            metrics: None,
            start_time: Utc::now(),
            end_time: None,
            tags: HashMap::new(),
            output: None,
        }
    }
    
    /// Marks the test as passed
    pub fn passed(mut self) -> Self {
        self.status = TestStatus::Passed;
        self.end_time = Some(Utc::now());
        self.duration = self.end_time.unwrap() - self.start_time;
        self
    }
    
    /// Marks the test as failed with an error message
    pub fn failed(mut self, error: String) -> Self {
        self.status = TestStatus::Failed;
        self.error = Some(error);
        self.end_time = Some(Utc::now());
        self.duration = self.end_time.unwrap() - self.start_time;
        self
    }
    
    /// Marks the test as skipped
    pub fn skipped(mut self) -> Self {
        self.status = TestStatus::Skipped;
        self.end_time = Some(Utc::now());
        self.duration = self.end_time.unwrap() - self.start_time;
        self
    }
    
    /// Adds metrics to the test result
    pub fn with_metrics(mut self, metrics: MetricsSnapshot) -> Self {
        self.metrics = Some(metrics);
        self
    }
    
    /// Adds a tag to the test result
    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }
    
    /// Adds output to the test result
    pub fn with_output(mut self, output: String) -> Self {
        self.output = Some(output);
        self
    }
}

/// Comprehensive test report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    /// Test suite name
    pub suite_name: String,
    /// Test execution start time
    pub start_time: DateTime<Utc>,
    /// Test execution end time
    pub end_time: Option<DateTime<Utc>>,
    /// Individual test results
    pub results: Vec<TestResult>,
    /// Aggregated metrics
    pub metrics: Option<MetricsSnapshot>,
    /// Code coverage data
    pub coverage: Option<CoverageData>,
    /// Performance summary
    pub performance_summary: Option<PerformanceSummary>,
    /// Environment information
    pub environment: HashMap<String, String>,
    /// Test configuration
    pub configuration: HashMap<String, serde_json::Value>,
    /// Report metadata
    pub metadata: ReportMetadata,
}

/// Report metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMetadata {
    /// Report generation timestamp
    pub generated_at: DateTime<Utc>,
    /// Report generator version
    pub generator_version: String,
    /// Test framework version
    pub framework_version: String,
    /// System information
    pub system_info: SystemInfo,
}

/// System information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Operating system
    pub os: String,
    /// OS version
    pub os_version: String,
    /// Architecture
    pub arch: String,
    /// Number of CPU cores
    pub cpu_cores: u32,
    /// Total memory in bytes
    pub total_memory_bytes: u64,
    /// Hostname
    pub hostname: String,
}

impl TestReport {
    /// Creates a new test report
    pub fn new(suite_name: String) -> Self {
        Self {
            suite_name,
            start_time: Utc::now(),
            end_time: None,
            results: Vec::new(),
            metrics: None,
            coverage: None,
            performance_summary: None,
            environment: Self::collect_environment(),
            configuration: HashMap::new(),
            metadata: ReportMetadata {
                generated_at: Utc::now(),
                generator_version: env!("CARGO_PKG_VERSION").to_string(),
                framework_version: "0.1.0".to_string(), // Would be dynamic
                system_info: Self::collect_system_info(),
            },
        }
    }
    
    /// Adds a test result to the report
    pub fn add_result(&mut self, result: TestResult) {
        info!("Adding test result: {} - {}", result.name, result.status);
        self.results.push(result);
    }
    
    /// Finalizes the report with end time and summary calculations
    pub fn finalize(&mut self) {
        self.end_time = Some(Utc::now());
        self.calculate_performance_summary();
        info!("Test report finalized for suite: {}", self.suite_name);
    }
    
    /// Gets test execution statistics
    pub fn get_stats(&self) -> TestStats {
        let total = self.results.len();
        let passed = self.results.iter().filter(|r| r.status == TestStatus::Passed).count();
        let failed = self.results.iter().filter(|r| r.status == TestStatus::Failed).count();
        let skipped = self.results.iter().filter(|r| r.status == TestStatus::Skipped).count();
        let timeout = self.results.iter().filter(|r| r.status == TestStatus::Timeout).count();
        
        let total_duration = if let Some(end_time) = self.end_time {
            end_time - self.start_time
        } else {
            Duration::ZERO
        };
        
        TestStats {
            total,
            passed,
            failed,
            skipped,
            timeout,
            success_rate: if total > 0 { (passed as f64 / total as f64) * 100.0 } else { 0.0 },
            total_duration,
        }
    }
    
    /// Generates HTML report
    pub fn generate_html(&self) -> String {
        let stats = self.get_stats();
        
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Test Report - {}</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        .header {{ background-color: #f4f4f4; padding: 20px; border-radius: 5px; }}
        .stats {{ display: flex; gap: 20px; margin: 20px 0; }}
        .stat-card {{ background: #fff; border: 1px solid #ddd; padding: 15px; border-radius: 5px; flex: 1; }}
        .passed {{ border-left: 4px solid #28a745; }}
        .failed {{ border-left: 4px solid #dc3545; }}
        .skipped {{ border-left: 4px solid #ffc107; }}
        .results {{ margin-top: 30px; }}
        .result {{ padding: 10px; margin: 5px 0; border-radius: 3px; }}
        .result.passed {{ background-color: #d4edda; }}
        .result.failed {{ background-color: #f8d7da; }}
        .result.skipped {{ background-color: #fff3cd; }}
        .performance {{ margin-top: 30px; background: #f8f9fa; padding: 20px; border-radius: 5px; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>Test Report: {}</h1>
        <p>Generated at: {}</p>
        <p>Duration: {:.2}s</p>
    </div>
    
    <div class="stats">
        <div class="stat-card passed">
            <h3>Passed</h3>
            <p>{}</p>
        </div>
        <div class="stat-card failed">
            <h3>Failed</h3>
            <p>{}</p>
        </div>
        <div class="stat-card skipped">
            <h3>Skipped</h3>
            <p>{}</p>
        </div>
        <div class="stat-card">
            <h3>Success Rate</h3>
            <p>{:.1}%</p>
        </div>
    </div>
    
    {}
    
    <div class="results">
        <h2>Test Results</h2>
        {}
    </div>
</body>
</html>"#,
            self.suite_name,
            self.suite_name,
            self.metadata.generated_at.format("%Y-%m-%d %H:%M:%S UTC"),
            stats.total_duration.as_secs_f64(),
            stats.passed,
            stats.failed,
            stats.skipped,
            stats.success_rate,
            self.generate_performance_html(),
            self.generate_results_html()
        )
    }
    
    /// Generates JSON report
    pub fn generate_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|e| {
            warn!("Failed to serialize test report to JSON: {}", e);
            serde_json::json!({
                "error": "Failed to serialize report",
                "suite_name": self.suite_name
            })
        })
    }
    
    /// Generates JUnit XML report
    pub fn generate_junit_xml(&self) -> String {
        let stats = self.get_stats();
        
        let mut xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="{}" tests="{}" failures="{}" skipped="{}" time="{:.3}">
"#,
            self.suite_name,
            stats.total,
            stats.failed,
            stats.skipped,
            stats.total_duration.as_secs_f64()
        );
        
        for result in &self.results {
            xml.push_str(&format!(
                r#"  <testcase classname="{}" name="{}" time="{:.3}""#,
                self.suite_name,
                result.name,
                result.duration.as_secs_f64()
            ));
            
            match result.status {
                TestStatus::Passed => {
                    xml.push_str(" />\n");
                }
                TestStatus::Failed => {
                    xml.push_str(">\n");
                    if let Some(error) = &result.error {
                        xml.push_str(&format!(
                            r#"    <failure message="{}">{}</failure>
"#,
                            error,
                            error
                        ));
                    }
                    xml.push_str("  </testcase>\n");
                }
                TestStatus::Skipped => {
                    xml.push_str(">\n    <skipped />\n  </testcase>\n");
                }
                TestStatus::Timeout => {
                    xml.push_str(">\n");
                    xml.push_str(r#"    <failure message="Test timeout">Test execution timed out</failure>
"#);
                    xml.push_str("  </testcase>\n");
                }
                TestStatus::Running => {
                    xml.push_str(">\n");
                    xml.push_str(r#"    <failure message="Test still running">Test was still running when report was generated</failure>
"#);
                    xml.push_str("  </testcase>\n");
                }
            }
        }
        
        xml.push_str("</testsuite>\n");
        xml
    }
    
    /// Exports metrics in Prometheus format
    pub fn export_prometheus(&self) -> String {
        let mut prometheus = String::new();
        let stats = self.get_stats();
        
        // Test suite metrics
        prometheus.push_str(&format!(
            "# HELP test_suite_total Total number of tests in the suite\n\
             # TYPE test_suite_total gauge\n\
             test_suite_total{{suite=\"{}\"}} {}\n\n",
            self.suite_name, stats.total
        ));
        
        prometheus.push_str(&format!(
            "# HELP test_suite_passed Number of passed tests\n\
             # TYPE test_suite_passed gauge\n\
             test_suite_passed{{suite=\"{}\"}} {}\n\n",
            self.suite_name, stats.passed
        ));
        
        prometheus.push_str(&format!(
            "# HELP test_suite_failed Number of failed tests\n\
             # TYPE test_suite_failed gauge\n\
             test_suite_failed{{suite=\"{}\"}} {}\n\n",
            self.suite_name, stats.failed
        ));
        
        prometheus.push_str(&format!(
            "# HELP test_suite_success_rate Success rate as percentage\n\
             # TYPE test_suite_success_rate gauge\n\
             test_suite_success_rate{{suite=\"{}\"}} {:.2}\n\n",
            self.suite_name, stats.success_rate
        ));
        
        prometheus.push_str(&format!(
            "# HELP test_suite_duration_seconds Total suite execution time\n\
             # TYPE test_suite_duration_seconds gauge\n\
             test_suite_duration_seconds{{suite=\"{}\"}} {:.3}\n\n",
            self.suite_name, stats.total_duration.as_secs_f64()
        ));
        
        // Performance metrics
        if let Some(perf) = &self.performance_summary {
            prometheus.push_str(&format!(
                "# HELP test_throughput_avg Average throughput in messages per second\n\
                 # TYPE test_throughput_avg gauge\n\
                 test_throughput_avg{{suite=\"{}\"}} {:.2}\n\n",
                self.suite_name, perf.avg_throughput
            ));
            
            prometheus.push_str(&format!(
                "# HELP test_latency_p99_ns P99 latency in nanoseconds\n\
                 # TYPE test_latency_p99_ns gauge\n\
                 test_latency_p99_ns{{suite=\"{}\"}} {}\n\n",
                self.suite_name, perf.p99_latency_ns
            ));
        }
        
        prometheus
    }
    
    /// Collects environment information
    fn collect_environment() -> HashMap<String, String> {
        let mut env = HashMap::new();
        
        // Common environment variables
        if let Ok(user) = std::env::var("USER") {
            env.insert("user".to_string(), user);
        }
        if let Ok(home) = std::env::var("HOME") {
            env.insert("home".to_string(), home);
        }
        if let Ok(path) = std::env::var("PATH") {
            env.insert("path".to_string(), path);
        }
        
        // CI/CD environment variables
        if let Ok(ci) = std::env::var("CI") {
            env.insert("ci".to_string(), ci);
        }
        if let Ok(github_actions) = std::env::var("GITHUB_ACTIONS") {
            env.insert("github_actions".to_string(), github_actions);
        }
        if let Ok(gitlab_ci) = std::env::var("GITLAB_CI") {
            env.insert("gitlab_ci".to_string(), gitlab_ci);
        }
        
        env
    }
    
    /// Collects system information
    fn collect_system_info() -> SystemInfo {
        SystemInfo {
            os: std::env::consts::OS.to_string(),
            os_version: "unknown".to_string(), // Would use system calls
            arch: std::env::consts::ARCH.to_string(),
            cpu_cores: num_cpus::get() as u32,
            total_memory_bytes: 0, // Would use system calls
            hostname: "unknown".to_string(), // Would use hostname command
        }
    }
    
    /// Calculates performance summary from test results
    fn calculate_performance_summary(&mut self) {
        let mut total_throughput = 0.0;
        let mut peak_throughput = 0.0;
        let mut total_latency = 0u64;
        let mut max_latency = 0u64;
        let mut total_messages = 0u64;
        let mut total_errors = 0u64;
        let mut valid_metrics_count = 0;
        
        for result in &self.results {
            if let Some(metrics) = &result.metrics {
                total_throughput += metrics.throughput;
                peak_throughput = peak_throughput.max(metrics.throughput);
                total_latency += metrics.latency_percentiles.p99 as u64;
                max_latency = max_latency.max(metrics.latency_percentiles.p99 as u64);
                
                // Sum error counts
                for count in metrics.error_counts.values() {
                    total_errors += count;
                }
                
                valid_metrics_count += 1;
            }
        }
        
        let avg_throughput = if valid_metrics_count > 0 {
            total_throughput / valid_metrics_count as f64
        } else {
            0.0
        };
        
        let avg_latency_ns = if valid_metrics_count > 0 {
            total_latency / valid_metrics_count as u64
        } else {
            0
        };
        
        let error_rate_percent = if total_messages > 0 {
            (total_errors as f64 / total_messages as f64) * 100.0
        } else {
            0.0
        };
        
        self.performance_summary = Some(PerformanceSummary {
            avg_throughput,
            peak_throughput,
            avg_latency_ns,
            p99_latency_ns: max_latency,
            total_messages,
            total_errors,
            error_rate_percent,
            resource_summary: ResourceSummary {
                peak_memory_bytes: 0,
                avg_cpu_percent: 0.0,
                peak_cpu_percent: 0.0,
                total_network_io_bytes: 0,
                total_disk_io_bytes: 0,
            },
        });
    }
    
    /// Generates performance section for HTML report
    fn generate_performance_html(&self) -> String {
        if let Some(perf) = &self.performance_summary {
            format!(
                r#"<div class="performance">
        <h2>Performance Summary</h2>
        <div class="stats">
            <div class="stat-card">
                <h3>Avg Throughput</h3>
                <p>{:.0} msg/s</p>
            </div>
            <div class="stat-card">
                <h3>Peak Throughput</h3>
                <p>{:.0} msg/s</p>
            </div>
            <div class="stat-card">
                <h3>P99 Latency</h3>
                <p>{:.2} ms</p>
            </div>
            <div class="stat-card">
                <h3>Error Rate</h3>
                <p>{:.2}%</p>
            </div>
        </div>
    </div>"#,
                perf.avg_throughput,
                perf.peak_throughput,
                perf.p99_latency_ns as f64 / 1_000_000.0, // Convert to ms
                perf.error_rate_percent
            )
        } else {
            String::new()
        }
    }
    
    /// Generates results section for HTML report
    fn generate_results_html(&self) -> String {
        let mut html = String::new();
        
        for result in &self.results {
            let status_class = match result.status {
                TestStatus::Passed => "passed",
                TestStatus::Failed => "failed",
                TestStatus::Skipped => "skipped",
                _ => "failed",
            };
            
            html.push_str(&format!(
                r#"<div class="result {}">
            <strong>{}</strong> - {} ({:.3}s)
            {}
        </div>"#,
                status_class,
                result.name,
                result.status,
                result.duration.as_secs_f64(),
                result.error.as_ref().map(|e| format!("<br><em>Error: {}</em>", e)).unwrap_or_default()
            ));
        }
        
        html
    }
}

/// Test execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStats {
    /// Total number of tests
    pub total: usize,
    /// Number of passed tests
    pub passed: usize,
    /// Number of failed tests
    pub failed: usize,
    /// Number of skipped tests
    pub skipped: usize,
    /// Number of timed out tests
    pub timeout: usize,
    /// Success rate as percentage
    pub success_rate: f64,
    /// Total execution duration
    pub total_duration: Duration,
}

/// Creates a test report builder for fluent configuration
pub fn create_test_report(suite_name: &str) -> TestReport {
    TestReport::new(suite_name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_report_creation() {
        let report = TestReport::new("test_suite".to_string());
        assert_eq!(report.suite_name, "test_suite");
        assert!(report.end_time.is_none());
    }
    
    #[test]
    fn test_result_creation() {
        let result = TestResult::new("test_case".to_string());
        assert_eq!(result.name, "test_case");
        assert_eq!(result.status, TestStatus::Running);
    }
    
    #[test]
    fn test_result_completion() {
        let result = TestResult::new("test_case".to_string())
            .passed()
            .with_tag("category".to_string(), "performance".to_string());
        
        assert_eq!(result.status, TestStatus::Passed);
        assert!(result.end_time.is_some());
        assert_eq!(result.tags.get("category"), Some(&"performance".to_string()));
    }
    
    #[test]
    fn test_junit_xml_generation() {
        let mut report = TestReport::new("test_suite".to_string());
        report.add_result(TestResult::new("test1".to_string()).passed());
        report.add_result(TestResult::new("test2".to_string()).failed("Test error".to_string()));
        report.finalize();
        
        let xml = report.generate_junit_xml();
        assert!(xml.contains("testsuite"));
        assert!(xml.contains("test1"));
        assert!(xml.contains("test2"));
        assert!(xml.contains("failure"));
    }
    
    #[test]
    fn test_stats_calculation() {
        let mut report = TestReport::new("test_suite".to_string());
        report.add_result(TestResult::new("test1".to_string()).passed());
        report.add_result(TestResult::new("test2".to_string()).failed("Error".to_string()));
        report.add_result(TestResult::new("test3".to_string()).skipped());
        
        let stats = report.get_stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.passed, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.skipped, 1);
        assert!((stats.success_rate - 33.33).abs() < 0.1);
    }
}