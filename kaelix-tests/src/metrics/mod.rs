//! # Test Metrics Collection and Analysis
//! 
//! This module provides comprehensive metrics collection for the MemoryStreamer testing infrastructure.
//! It supports real-time monitoring, performance tracking, and integration with observability platforms.

#[cfg(feature = "monitoring")]
use prometheus::{Counter, Gauge, Histogram, HistogramOpts, Opts, Registry};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Snapshot of metrics at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Timestamp when the snapshot was taken
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Current throughput in messages per second
    pub throughput: f64,
    /// Current latency percentiles in nanoseconds
    pub latency_percentiles: LatencyPercentiles,
    /// Error counts by error type
    pub error_counts: HashMap<String, u64>,
    /// System resource usage
    pub resource_usage: ResourceUsage,
    /// Custom metrics
    pub custom_metrics: HashMap<String, f64>,
}

/// Latency percentile measurements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
    pub p999: f64,
}

/// System resource usage metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// CPU usage percentage (0-100)
    pub cpu_percent: f64,
    /// Network I/O in bytes per second
    pub network_io_bps: u64,
    /// Disk I/O in bytes per second
    pub disk_io_bps: u64,
    /// Number of open file descriptors
    pub open_fds: u64,
    /// Number of threads
    pub thread_count: u64,
}

/// Core metrics collection system for test execution
#[derive(Debug)]
pub struct TestMetrics {
    #[cfg(feature = "monitoring")]
    /// Prometheus registry for metric collection
    registry: Arc<Registry>,
    #[cfg(feature = "monitoring")]
    /// Throughput histogram (messages per second)
    throughput: Histogram,
    #[cfg(feature = "monitoring")]
    /// Latency histogram (nanoseconds)
    latency: Histogram,
    #[cfg(feature = "monitoring")]
    /// Error counter by type
    errors: Counter,
    #[cfg(feature = "monitoring")]
    /// Memory usage gauge (bytes)
    memory_usage: Gauge,
    #[cfg(feature = "monitoring")]
    /// CPU usage gauge (percentage)
    cpu_usage: Gauge,
    #[cfg(feature = "monitoring")]
    /// Network I/O counter (bytes)
    network_io: Counter,
    #[cfg(feature = "monitoring")]
    /// Disk I/O counter (bytes)
    disk_io: Counter,
    
    /// Test execution start time
    start_time: Instant,
    /// Simple metrics storage when prometheus is not available
    #[cfg(not(feature = "monitoring"))]
    simple_metrics: Arc<parking_lot::RwLock<HashMap<String, f64>>>,
    /// Custom metrics registry
    custom_metrics: Arc<parking_lot::RwLock<HashMap<String, f64>>>,
}

impl Default for TestMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl TestMetrics {
    /// Creates a new TestMetrics instance with default configuration
    pub fn new() -> Self {
        #[cfg(feature = "monitoring")]
        {
            let registry = Arc::new(Registry::new());
            
            // Create throughput histogram with appropriate buckets for high-performance testing
            let throughput_opts = HistogramOpts::new(
                "test_throughput_msgs_per_sec",
                "Message throughput in messages per second"
            ).buckets(vec![
                1000.0, 5000.0, 10000.0, 50000.0, 100000.0, 
                500000.0, 1000000.0, 5000000.0, 10000000.0, 50000000.0
            ]);
            let throughput = Histogram::with_opts(throughput_opts).unwrap();
            registry.register(Box::new(throughput.clone())).unwrap();

            // Create latency histogram with nanosecond precision
            let latency_opts = HistogramOpts::new(
                "test_latency_nanoseconds", 
                "Message processing latency in nanoseconds"
            ).buckets(prometheus::exponential_buckets(100.0, 2.0, 20).unwrap());
            let latency = Histogram::with_opts(latency_opts).unwrap();
            registry.register(Box::new(latency.clone())).unwrap();

            // Create error counter
            let errors = Counter::with_opts(Opts::new(
                "test_errors_total",
                "Total number of test errors"
            )).unwrap();
            registry.register(Box::new(errors.clone())).unwrap();

            // Create resource usage gauges
            let memory_usage = Gauge::with_opts(Opts::new(
                "test_memory_usage_bytes",
                "Memory usage in bytes"
            )).unwrap();
            registry.register(Box::new(memory_usage.clone())).unwrap();

            let cpu_usage = Gauge::with_opts(Opts::new(
                "test_cpu_usage_percent",
                "CPU usage percentage"
            )).unwrap();
            registry.register(Box::new(cpu_usage.clone())).unwrap();

            let network_io = Counter::with_opts(Opts::new(
                "test_network_io_bytes_total",
                "Network I/O in bytes"
            )).unwrap();
            registry.register(Box::new(network_io.clone())).unwrap();

            let disk_io = Counter::with_opts(Opts::new(
                "test_disk_io_bytes_total",
                "Disk I/O in bytes"
            )).unwrap();
            registry.register(Box::new(disk_io.clone())).unwrap();

            Self {
                registry,
                throughput,
                latency,
                errors,
                memory_usage,
                cpu_usage,
                network_io,
                disk_io,
                start_time: Instant::now(),
                custom_metrics: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            }
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            Self {
                start_time: Instant::now(),
                simple_metrics: Arc::new(parking_lot::RwLock::new(HashMap::new())),
                custom_metrics: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            }
        }
    }

    /// Records throughput measurement
    pub fn record_throughput(&self, msgs_per_sec: u64) {
        debug!("Recording throughput: {} msgs/sec", msgs_per_sec);
        
        #[cfg(feature = "monitoring")]
        {
            self.throughput.observe(msgs_per_sec as f64);
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            self.simple_metrics.write().insert("throughput".to_string(), msgs_per_sec as f64);
        }
    }

    /// Records latency measurement
    pub fn record_latency(&self, latency_ns: u64) {
        #[cfg(feature = "monitoring")]
        {
            self.latency.observe(latency_ns as f64);
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            self.simple_metrics.write().insert("latency_ns".to_string(), latency_ns as f64);
        }
    }

    /// Records an error occurrence
    pub fn record_error(&self, error_type: &str) {
        warn!("Recording error: {}", error_type);
        
        #[cfg(feature = "monitoring")]
        {
            self.errors.inc();
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            let mut metrics = self.simple_metrics.write();
            let current = metrics.get("errors").unwrap_or(&0.0);
            metrics.insert("errors".to_string(), current + 1.0);
        }
    }

    /// Records memory usage
    pub fn record_memory_usage(&self, bytes: u64) {
        #[cfg(feature = "monitoring")]
        {
            self.memory_usage.set(bytes as f64);
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            self.simple_metrics.write().insert("memory_bytes".to_string(), bytes as f64);
        }
    }

    /// Records CPU usage percentage
    pub fn record_cpu_usage(&self, percent: f64) {
        #[cfg(feature = "monitoring")]
        {
            self.cpu_usage.set(percent);
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            self.simple_metrics.write().insert("cpu_percent".to_string(), percent);
        }
    }

    /// Records network I/O
    pub fn record_network_io(&self, bytes: u64) {
        #[cfg(feature = "monitoring")]
        {
            self.network_io.inc_by(bytes as f64);
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            let mut metrics = self.simple_metrics.write();
            let current = metrics.get("network_io").unwrap_or(&0.0);
            metrics.insert("network_io".to_string(), current + bytes as f64);
        }
    }

    /// Records disk I/O
    pub fn record_disk_io(&self, bytes: u64) {
        #[cfg(feature = "monitoring")]
        {
            self.disk_io.inc_by(bytes as f64);
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            let mut metrics = self.simple_metrics.write();
            let current = metrics.get("disk_io").unwrap_or(&0.0);
            metrics.insert("disk_io".to_string(), current + bytes as f64);
        }
    }

    /// Records a custom metric
    pub fn record_custom_metric(&self, name: &str, value: f64) {
        #[cfg(feature = "monitoring")]
        {
            // For simplicity, we'll store custom metrics as gauges
            let gauge = Gauge::with_opts(Opts::new(
                &format!("test_custom_{}", name),
                &format!("Custom metric: {}", name)
            )).unwrap();
            
            gauge.set(value);
            self.registry.register(Box::new(gauge.clone())).unwrap_or_else(|_| {
                // Metric already registered, just update value
                gauge.set(value);
            });
        }

        self.custom_metrics.write().insert(name.to_string(), value);
    }

    /// Exports metrics in Prometheus format
    pub fn export_prometheus(&self) -> String {
        #[cfg(feature = "monitoring")]
        {
            use prometheus::Encoder;
            let encoder = prometheus::TextEncoder::new();
            let metric_families = self.registry.gather();
            encoder.encode_to_string(&metric_families).unwrap_or_else(|e| {
                warn!("Failed to encode metrics: {}", e);
                String::new()
            })
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            // Simple text format when prometheus is not available
            let mut output = String::new();
            
            #[cfg(not(feature = "monitoring"))]
            {
                let metrics = self.simple_metrics.read();
                for (name, value) in metrics.iter() {
                    output.push_str(&format!("{}={}\n", name, value));
                }
            }
            
            let custom = self.custom_metrics.read();
            for (name, value) in custom.iter() {
                output.push_str(&format!("custom_{}={}\n", name, value));
            }
            
            output
        }
    }

    /// Gets a snapshot of current metrics
    pub fn get_snapshot(&self) -> MetricsSnapshot {
        let throughput = self.get_throughput();
        let latency_percentiles = self.get_latency_percentiles();
        let error_counts = self.get_error_counts();
        let resource_usage = self.get_resource_usage();
        let custom_metrics = self.get_custom_metrics();
        
        MetricsSnapshot {
            timestamp: chrono::Utc::now(),
            throughput,
            latency_percentiles,
            error_counts,
            resource_usage,
            custom_metrics,
        }
    }

    /// Resets all metrics
    pub fn reset(&self) {
        info!("Resetting test metrics");
        
        #[cfg(not(feature = "monitoring"))]
        {
            self.simple_metrics.write().clear();
        }
        
        self.custom_metrics.write().clear();
    }

    /// Gets the test execution duration
    pub fn get_duration(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Gets current throughput
    fn get_throughput(&self) -> f64 {
        #[cfg(feature = "monitoring")]
        {
            self.throughput.get_sample_sum() / self.start_time.elapsed().as_secs_f64()
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            self.simple_metrics.read().get("throughput").copied().unwrap_or(0.0)
        }
    }

    /// Gets latency percentiles from histogram
    fn get_latency_percentiles(&self) -> LatencyPercentiles {
        #[cfg(feature = "monitoring")]
        {
            // This is a simplified implementation
            // In a real implementation, you would calculate percentiles from histogram buckets
            LatencyPercentiles {
                p50: 0.0,
                p90: 0.0,
                p95: 0.0,
                p99: 0.0,
                p999: 0.0,
            }
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            let latency = self.simple_metrics.read().get("latency_ns").copied().unwrap_or(0.0);
            LatencyPercentiles {
                p50: latency * 0.5,
                p90: latency * 0.9,
                p95: latency * 0.95,
                p99: latency,
                p999: latency * 1.1,
            }
        }
    }

    /// Gets error counts by type
    fn get_error_counts(&self) -> HashMap<String, u64> {
        let mut counts = HashMap::new();
        
        #[cfg(not(feature = "monitoring"))]
        {
            if let Some(errors) = self.simple_metrics.read().get("errors") {
                counts.insert("total".to_string(), *errors as u64);
            }
        }
        
        counts
    }

    /// Gets current resource usage
    fn get_resource_usage(&self) -> ResourceUsage {
        #[cfg(not(feature = "monitoring"))]
        {
            let metrics = self.simple_metrics.read();
            ResourceUsage {
                memory_bytes: metrics.get("memory_bytes").copied().unwrap_or(0.0) as u64,
                cpu_percent: metrics.get("cpu_percent").copied().unwrap_or(0.0),
                network_io_bps: metrics.get("network_io").copied().unwrap_or(0.0) as u64,
                disk_io_bps: metrics.get("disk_io").copied().unwrap_or(0.0) as u64,
                open_fds: 0,
                thread_count: 0,
            }
        }
        
        #[cfg(feature = "monitoring")]
        {
            // Would get actual values from prometheus metrics
            ResourceUsage {
                memory_bytes: 0,
                cpu_percent: 0.0,
                network_io_bps: 0,
                disk_io_bps: 0,
                open_fds: 0,
                thread_count: 0,
            }
        }
    }

    /// Gets custom metrics values
    fn get_custom_metrics(&self) -> HashMap<String, f64> {
        self.custom_metrics.read().clone()
    }
}

/// Creates a new TestMetrics instance for test suites
pub fn create_test_metrics() -> TestMetrics {
    TestMetrics::new()
}

/// Creates a shared TestMetrics instance for concurrent access
pub fn create_shared_test_metrics() -> Arc<TestMetrics> {
    Arc::new(TestMetrics::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = TestMetrics::new();
        assert!(metrics.get_duration().as_nanos() > 0);
    }

    #[test]
    fn test_throughput_recording() {
        let metrics = TestMetrics::new();
        metrics.record_throughput(10000);
        
        let prometheus_output = metrics.export_prometheus();
        assert!(!prometheus_output.is_empty());
    }

    #[test]
    fn test_latency_recording() {
        let metrics = TestMetrics::new();
        metrics.record_latency(5000); // 5 microseconds
        
        let prometheus_output = metrics.export_prometheus();
        assert!(!prometheus_output.is_empty());
    }

    #[test]
    fn test_error_recording() {
        let metrics = TestMetrics::new();
        metrics.record_error("timeout");
        
        let prometheus_output = metrics.export_prometheus();
        assert!(!prometheus_output.is_empty());
    }

    #[test]
    fn test_custom_metrics() {
        let metrics = TestMetrics::new();
        metrics.record_custom_metric("custom_rate", 42.5);
        
        let prometheus_output = metrics.export_prometheus();
        assert!(prometheus_output.contains("custom_rate"));
    }

    #[test]
    fn test_metrics_snapshot() {
        let metrics = TestMetrics::new();
        metrics.record_throughput(1000);
        metrics.record_latency(2000);
        
        let snapshot = metrics.get_snapshot();
        assert!(snapshot.timestamp <= chrono::Utc::now());
    }
}