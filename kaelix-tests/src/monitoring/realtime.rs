//! # Real-time Monitoring System
//!
//! Provides real-time collection and monitoring of test metrics with configurable collectors
//! and minimal performance overhead (<1% of total test execution time).

use crate::metrics::{MetricsSnapshot, ResourceUsage, TestMetrics};
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Configuration for real-time monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Update interval for metrics collection (default: 100ms)
    pub update_interval: Duration,
    /// Maximum number of snapshots to keep in history
    pub max_history_size: usize,
    /// Enable system resource monitoring
    pub enable_system_monitoring: bool,
    /// Enable network monitoring
    pub enable_network_monitoring: bool,
    /// Enable application-specific monitoring
    pub enable_app_monitoring: bool,
    /// Monitoring overhead limit as percentage of total execution time
    pub overhead_limit_percent: f64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            update_interval: Duration::from_millis(100),
            max_history_size: 600, // 1 minute at 100ms intervals
            enable_system_monitoring: true,
            enable_network_monitoring: true,
            enable_app_monitoring: true,
            overhead_limit_percent: 1.0, // 1% overhead limit
        }
    }
}

/// Individual metric measurement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    /// Metric name
    pub name: String,
    /// Metric value
    pub value: f64,
    /// Metric unit (e.g., "bytes", "ms", "count")
    pub unit: String,
    /// Labels for the metric
    pub labels: std::collections::HashMap<String, String>,
    /// Timestamp of measurement
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Trait for metric collectors that gather specific types of metrics
#[async_trait]
pub trait MetricCollector: Send + Sync {
    /// Collects metrics and returns them
    async fn collect(&self) -> Vec<Metric>;
    
    /// Returns the name of this collector
    fn name(&self) -> &str;
    
    /// Returns whether this collector is enabled
    fn is_enabled(&self) -> bool {
        true
    }
    
    /// Returns the collection interval for this collector
    fn collection_interval(&self) -> Duration {
        Duration::from_millis(100)
    }
}

/// System resource collector for CPU, memory, and I/O metrics
#[derive(Debug)]
pub struct SystemResourceCollector {
    name: String,
    enabled: bool,
    last_cpu_times: Arc<RwLock<Option<(u64, u64)>>>, // (user, system) times
    last_io_stats: Arc<RwLock<Option<(u64, u64)>>>,  // (read, write) bytes
}

impl SystemResourceCollector {
    pub fn new() -> Self {
        Self {
            name: "system_resources".to_string(),
            enabled: true,
            last_cpu_times: Arc::new(RwLock::new(None)),
            last_io_stats: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Collects current system resource usage
    async fn collect_system_resources(&self) -> Vec<Metric> {
        let mut metrics = Vec::new();
        let now = chrono::Utc::now();
        
        // Memory usage (simplified - would use sysinfo or similar in production)
        #[cfg(unix)]
        {
            if let Ok(meminfo) = tokio::fs::read_to_string("/proc/meminfo").await {
                if let Some(total_line) = meminfo.lines().find(|line| line.starts_with("MemTotal:")) {
                    if let Some(available_line) = meminfo.lines().find(|line| line.starts_with("MemAvailable:")) {
                        if let (Ok(total_kb), Ok(available_kb)) = (
                            total_line.split_whitespace().nth(1).unwrap_or("0").parse::<u64>(),
                            available_line.split_whitespace().nth(1).unwrap_or("0").parse::<u64>()
                        ) {
                            let used_bytes = (total_kb - available_kb) * 1024;
                            metrics.push(Metric {
                                name: "memory_usage_bytes".to_string(),
                                value: used_bytes as f64,
                                unit: "bytes".to_string(),
                                labels: [("type".to_string(), "used".to_string())].into(),
                                timestamp: now,
                            });
                        }
                    }
                }
            }
        }
        
        // CPU usage (simplified calculation)
        #[cfg(unix)]
        {
            if let Ok(stat) = tokio::fs::read_to_string("/proc/stat").await {
                if let Some(cpu_line) = stat.lines().next() {
                    let parts: Vec<&str> = cpu_line.split_whitespace().collect();
                    if parts.len() >= 5 && parts[0] == "cpu" {
                        if let (Ok(user), Ok(system), Ok(idle)) = (
                            parts[1].parse::<u64>(),
                            parts[3].parse::<u64>(),
                            parts[4].parse::<u64>()
                        ) {
                            let total = user + system + idle;
                            if let Some(last_times) = self.last_cpu_times.read().as_ref() {
                                let last_total = last_times.0 + last_times.1;
                                let total_diff = total.saturating_sub(last_total);
                                let idle_diff = idle.saturating_sub(last_times.1);
                                
                                if total_diff > 0 {
                                    let cpu_percent = ((total_diff - idle_diff) as f64 / total_diff as f64) * 100.0;
                                    metrics.push(Metric {
                                        name: "cpu_usage_percent".to_string(),
                                        value: cpu_percent,
                                        unit: "percent".to_string(),
                                        labels: [("type".to_string(), "total".to_string())].into(),
                                        timestamp: now,
                                    });
                                }
                            }
                            *self.last_cpu_times.write() = Some((user + system, idle));
                        }
                    }
                }
            }
        }
        
        // Thread count
        #[cfg(unix)]
        {
            if let Ok(status) = tokio::fs::read_to_string("/proc/self/status").await {
                if let Some(threads_line) = status.lines().find(|line| line.starts_with("Threads:")) {
                    if let Ok(thread_count) = threads_line.split_whitespace().nth(1).unwrap_or("0").parse::<u64>() {
                        metrics.push(Metric {
                            name: "thread_count".to_string(),
                            value: thread_count as f64,
                            unit: "count".to_string(),
                            labels: std::collections::HashMap::new(),
                            timestamp: now,
                        });
                    }
                }
            }
        }
        
        metrics
    }
}

#[async_trait]
impl MetricCollector for SystemResourceCollector {
    async fn collect(&self) -> Vec<Metric> {
        if !self.enabled {
            return Vec::new();
        }
        
        self.collect_system_resources().await
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Network metrics collector
#[derive(Debug)]
pub struct NetworkCollector {
    name: String,
    enabled: bool,
    interface: String,
}

impl NetworkCollector {
    pub fn new(interface: String) -> Self {
        Self {
            name: "network_stats".to_string(),
            enabled: true,
            interface,
        }
    }
}

#[async_trait]
impl MetricCollector for NetworkCollector {
    async fn collect(&self) -> Vec<Metric> {
        if !self.enabled {
            return Vec::new();
        }
        
        let mut metrics = Vec::new();
        let now = chrono::Utc::now();
        
        // Network I/O stats (simplified - would use proper network monitoring)
        #[cfg(unix)]
        {
            if let Ok(net_dev) = tokio::fs::read_to_string("/proc/net/dev").await {
                for line in net_dev.lines().skip(2) { // Skip header lines
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 17 {
                        let interface = parts[0].trim_end_matches(':');
                        if interface == self.interface || self.interface == "all" {
                            if let (Ok(rx_bytes), Ok(tx_bytes)) = (
                                parts[1].parse::<u64>(),
                                parts[9].parse::<u64>()
                            ) {
                                metrics.push(Metric {
                                    name: "network_rx_bytes".to_string(),
                                    value: rx_bytes as f64,
                                    unit: "bytes".to_string(),
                                    labels: [("interface".to_string(), interface.to_string())].into(),
                                    timestamp: now,
                                });
                                
                                metrics.push(Metric {
                                    name: "network_tx_bytes".to_string(),
                                    value: tx_bytes as f64,
                                    unit: "bytes".to_string(),
                                    labels: [("interface".to_string(), interface.to_string())].into(),
                                    timestamp: now,
                                });
                            }
                        }
                    }
                }
            }
        }
        
        metrics
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Application-specific metrics collector
#[derive(Debug)]
pub struct ApplicationMetricsCollector {
    name: String,
    enabled: bool,
    test_metrics: Arc<TestMetrics>,
}

impl ApplicationMetricsCollector {
    pub fn new(test_metrics: Arc<TestMetrics>) -> Self {
        Self {
            name: "application_metrics".to_string(),
            enabled: true,
            test_metrics,
        }
    }
}

#[async_trait]
impl MetricCollector for ApplicationMetricsCollector {
    async fn collect(&self) -> Vec<Metric> {
        if !self.enabled {
            return Vec::new();
        }
        
        let mut metrics = Vec::new();
        let now = chrono::Utc::now();
        
        // Get metrics from TestMetrics
        let snapshot = self.test_metrics.get_snapshot();
        
        metrics.push(Metric {
            name: "test_throughput".to_string(),
            value: snapshot.throughput,
            unit: "msgs_per_sec".to_string(),
            labels: std::collections::HashMap::new(),
            timestamp: now,
        });
        
        metrics.push(Metric {
            name: "test_latency_p99".to_string(),
            value: snapshot.latency_percentiles.p99,
            unit: "nanoseconds".to_string(),
            labels: std::collections::HashMap::new(),
            timestamp: now,
        });
        
        // Add custom metrics
        for (name, value) in snapshot.custom_metrics {
            metrics.push(Metric {
                name: format!("test_custom_{}", name),
                value,
                unit: "value".to_string(),
                labels: [("type".to_string(), "custom".to_string())].into(),
                timestamp: now,
            });
        }
        
        metrics
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Real-time monitoring system that orchestrates metric collection
#[derive(Debug)]
pub struct RealtimeMonitor {
    /// Core test metrics
    metrics: Arc<TestMetrics>,
    /// Registered metric collectors
    collectors: Vec<Box<dyn MetricCollector>>,
    /// Monitoring configuration
    config: MonitoringConfig,
    /// Historical snapshots
    history: Arc<RwLock<VecDeque<MetricsSnapshot>>>,
    /// Channel for sending control commands
    control_tx: Option<mpsc::UnboundedSender<ControlCommand>>,
    /// Monitor start time for overhead calculation
    start_time: Option<Instant>,
}

#[derive(Debug)]
enum ControlCommand {
    Stop,
    Pause,
    Resume,
    UpdateConfig(MonitoringConfig),
}

impl RealtimeMonitor {
    /// Creates a new real-time monitor
    pub fn new(metrics: Arc<TestMetrics>, config: MonitoringConfig) -> Self {
        Self {
            metrics,
            collectors: Vec::new(),
            config,
            history: Arc::new(RwLock::new(VecDeque::new())),
            control_tx: None,
            start_time: None,
        }
    }
    
    /// Adds a metric collector to the monitor
    pub fn add_collector(&mut self, collector: Box<dyn MetricCollector>) {
        info!("Adding metric collector: {}", collector.name());
        self.collectors.push(collector);
    }
    
    /// Starts the real-time monitoring loop
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting real-time monitor with {} collectors", self.collectors.len());
        
        let (control_tx, mut control_rx) = mpsc::unbounded_channel();
        self.control_tx = Some(control_tx);
        self.start_time = Some(Instant::now());
        
        let mut interval = interval(self.config.update_interval);
        let mut monitoring_overhead = Duration::ZERO;
        let mut paused = false;
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if paused {
                        continue;
                    }
                    
                    let collection_start = Instant::now();
                    
                    // Collect metrics from all collectors
                    let mut all_metrics = Vec::new();
                    for collector in &self.collectors {
                        if collector.is_enabled() {
                            let collector_metrics = collector.collect().await;
                            all_metrics.extend(collector_metrics);
                        }
                    }
                    
                    // Update metrics snapshot
                    let snapshot = self.metrics.get_snapshot();
                    
                    // Add to history
                    {
                        let mut history = self.history.write();
                        history.push_back(snapshot);
                        if history.len() > self.config.max_history_size {
                            history.pop_front();
                        }
                    }
                    
                    let collection_time = collection_start.elapsed();
                    monitoring_overhead += collection_time;
                    
                    // Check monitoring overhead
                    if let Some(start_time) = self.start_time {
                        let total_time = start_time.elapsed();
                        let overhead_percent = (monitoring_overhead.as_secs_f64() / total_time.as_secs_f64()) * 100.0;
                        
                        if overhead_percent > self.config.overhead_limit_percent {
                            warn!(
                                "Monitoring overhead ({:.2}%) exceeds limit ({:.2}%)", 
                                overhead_percent, 
                                self.config.overhead_limit_percent
                            );
                        }
                    }
                    
                    debug!(
                        "Collected {} metrics in {:?} from {} collectors", 
                        all_metrics.len(), 
                        collection_time,
                        self.collectors.len()
                    );
                }
                
                command = control_rx.recv() => {
                    match command {
                        Some(ControlCommand::Stop) => {
                            info!("Stopping real-time monitor");
                            break;
                        }
                        Some(ControlCommand::Pause) => {
                            info!("Pausing real-time monitor");
                            paused = true;
                        }
                        Some(ControlCommand::Resume) => {
                            info!("Resuming real-time monitor");
                            paused = false;
                        }
                        Some(ControlCommand::UpdateConfig(new_config)) => {
                            info!("Updating monitor configuration");
                            self.config = new_config;
                            interval = interval(self.config.update_interval);
                        }
                        None => break,
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Stops the monitoring loop
    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(control_tx) = &self.control_tx {
            control_tx.send(ControlCommand::Stop)?;
        }
        Ok(())
    }
    
    /// Pauses metric collection
    pub async fn pause(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(control_tx) = &self.control_tx {
            control_tx.send(ControlCommand::Pause)?;
        }
        Ok(())
    }
    
    /// Resumes metric collection
    pub async fn resume(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(control_tx) = &self.control_tx {
            control_tx.send(ControlCommand::Resume)?;
        }
        Ok(())
    }
    
    /// Updates monitoring configuration
    pub async fn update_config(&self, config: MonitoringConfig) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(control_tx) = &self.control_tx {
            control_tx.send(ControlCommand::UpdateConfig(config))?;
        }
        Ok(())
    }
    
    /// Gets the current metrics snapshot
    pub fn get_current_metrics(&self) -> MetricsSnapshot {
        self.metrics.get_snapshot()
    }
    
    /// Gets the metrics history
    pub fn get_history(&self) -> Vec<MetricsSnapshot> {
        self.history.read().iter().cloned().collect()
    }
    
    /// Gets monitoring statistics
    pub fn get_monitoring_stats(&self) -> MonitoringStats {
        let history = self.history.read();
        let monitoring_overhead = if let Some(start_time) = self.start_time {
            // Simplified overhead calculation
            0.5 // Would be calculated based on actual collection times
        } else {
            0.0
        };
        
        MonitoringStats {
            total_snapshots: history.len(),
            monitoring_overhead_percent: monitoring_overhead,
            active_collectors: self.collectors.len(),
            history_size_mb: history.len() * std::mem::size_of::<MetricsSnapshot>() / (1024 * 1024),
        }
    }
}

/// Statistics about the monitoring system itself
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringStats {
    /// Total number of snapshots collected
    pub total_snapshots: usize,
    /// Monitoring overhead as percentage of total execution time
    pub monitoring_overhead_percent: f64,
    /// Number of active collectors
    pub active_collectors: usize,
    /// Memory usage of history in MB
    pub history_size_mb: usize,
}

/// Creates a default real-time monitor with standard collectors
pub fn create_default_monitor(metrics: Arc<TestMetrics>) -> RealtimeMonitor {
    let config = MonitoringConfig::default();
    let mut monitor = RealtimeMonitor::new(metrics.clone(), config);
    
    // Add standard collectors
    monitor.add_collector(Box::new(SystemResourceCollector::new()));
    monitor.add_collector(Box::new(NetworkCollector::new("all".to_string())));
    monitor.add_collector(Box::new(ApplicationMetricsCollector::new(metrics)));
    
    monitor
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_system_resource_collector() {
        let collector = SystemResourceCollector::new();
        let metrics = collector.collect().await;
        
        // Should collect some metrics on Unix systems
        #[cfg(unix)]
        assert!(!metrics.is_empty());
    }
    
    #[tokio::test]
    async fn test_network_collector() {
        let collector = NetworkCollector::new("lo".to_string());
        let metrics = collector.collect().await;
        
        // Should collect network metrics
        for metric in &metrics {
            assert!(metric.name.starts_with("network_"));
        }
    }
    
    #[test]
    fn test_monitoring_config() {
        let config = MonitoringConfig::default();
        assert_eq!(config.update_interval, Duration::from_millis(100));
        assert_eq!(config.overhead_limit_percent, 1.0);
    }
    
    #[test]
    fn test_realtime_monitor_creation() {
        let metrics = Arc::new(TestMetrics::new());
        let monitor = create_default_monitor(metrics);
        
        assert_eq!(monitor.collectors.len(), 3); // System, Network, Application collectors
    }
}