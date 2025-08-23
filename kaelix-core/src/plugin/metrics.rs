//! Plugin performance metrics and monitoring.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Plugin performance monitoring and metrics collection.
///
/// Provides comprehensive metrics collection for plugin operations including
/// invocation latency, throughput, error rates, and resource usage.
///
/// # Features
///
/// - Real-time performance monitoring
/// - Configurable metrics collection intervals
/// - Memory and CPU usage tracking
/// - Custom plugin-specific metrics
/// - Historical data aggregation
/// - Efficient atomic counters for hot paths
///
/// # Examples
///
/// ```rust
/// use kaelix_core::plugin::*;
/// use std::time::Duration;
///
/// let monitor = PluginMonitor::new();
///
/// // Track plugin invocation
/// let result = monitor.track_invocation(plugin_id, async {
///     // Plugin operation here
///     42
/// }).await;
///
/// // Collect metrics
/// let metrics = monitor.collect_metrics(plugin_id).await;
/// ```
pub struct PluginMonitor {
    /// Metrics collector configuration
    config: MonitorConfig,

    /// Per-plugin metrics storage
    plugin_metrics: Arc<RwLock<HashMap<crate::plugin::PluginId, PluginMetrics>>>,

    /// Global metrics aggregation
    global_metrics: GlobalMetrics,

    /// Monitor creation timestamp
    created_at: Instant,
}

impl PluginMonitor {
    /// Create a new plugin monitor with default configuration.
    pub fn new() -> Self {
        Self::with_config(MonitorConfig::default())
    }

    /// Create a new plugin monitor with custom configuration.
    pub fn with_config(config: MonitorConfig) -> Self {
        Self {
            config,
            plugin_metrics: Arc::new(RwLock::new(HashMap::new())),
            global_metrics: GlobalMetrics::new(),
            created_at: Instant::now(),
        }
    }

    /// Track a plugin invocation and collect metrics.
    ///
    /// # Parameters
    /// - `plugin_id`: ID of the plugin being invoked
    /// - `operation`: Async operation to track
    ///
    /// # Returns
    /// - `T`: Result of the plugin operation
    pub async fn track_invocation<T, F>(
        &self,
        plugin_id: crate::plugin::PluginId,
        operation: F,
    ) -> T
    where
        F: std::future::Future<Output = T>,
    {
        let start_time = Instant::now();

        // Execute the operation
        let result = operation.await;

        // Record metrics
        let duration = start_time.elapsed();
        self.record_invocation(plugin_id, duration, true).await;

        result
    }

    /// Track a plugin invocation that may fail.
    pub async fn track_invocation_result<T, E, F>(
        &self,
        plugin_id: crate::plugin::PluginId,
        operation: F,
    ) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        let start_time = Instant::now();

        // Execute the operation
        let result = operation.await;

        // Record metrics
        let duration = start_time.elapsed();
        let success = result.is_ok();
        self.record_invocation(plugin_id, duration, success).await;

        result
    }

    /// Record a plugin invocation manually.
    async fn record_invocation(
        &self,
        plugin_id: crate::plugin::PluginId,
        duration: Duration,
        success: bool,
    ) {
        // Update plugin-specific metrics
        {
            let mut metrics_map = self.plugin_metrics.write().await;
            let metrics = metrics_map.entry(plugin_id).or_insert_with(PluginMetrics::new);
            metrics.record_invocation(duration, success).await;
        }

        // Update global metrics
        self.global_metrics.record_invocation(duration, success);
    }

    /// Collect current metrics for a specific plugin.
    pub async fn collect_metrics(
        &self,
        plugin_id: crate::plugin::PluginId,
    ) -> Option<PluginMetricsSnapshot> {
        let metrics_map = self.plugin_metrics.read().await;
        if let Some(metrics) = metrics_map.get(&plugin_id) {
            Some(metrics.to_snapshot().await)
        } else {
            None
        }
    }

    /// Collect metrics for all monitored plugins.
    pub async fn collect_all_metrics(
        &self,
    ) -> HashMap<crate::plugin::PluginId, PluginMetricsSnapshot> {
        let metrics_map = self.plugin_metrics.read().await;
        let mut result = HashMap::new();

        for (plugin_id, metrics) in metrics_map.iter() {
            result.insert(*plugin_id, metrics.to_snapshot().await);
        }

        result
    }

    /// Get global aggregated metrics.
    pub fn global_metrics(&self) -> &GlobalMetrics {
        &self.global_metrics
    }

    /// Get monitor configuration.
    pub fn config(&self) -> &MonitorConfig {
        &self.config
    }

    /// Get monitor uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Clear metrics for a specific plugin.
    pub async fn clear_plugin_metrics(&self, plugin_id: crate::plugin::PluginId) {
        let mut metrics_map = self.plugin_metrics.write().await;
        metrics_map.remove(&plugin_id);
    }

    /// Clear all collected metrics.
    pub async fn clear_all_metrics(&self) {
        let mut metrics_map = self.plugin_metrics.write().await;
        metrics_map.clear();
        self.global_metrics.reset();
    }

    /// Check if metrics are being collected for a plugin.
    pub async fn is_monitoring(&self, plugin_id: crate::plugin::PluginId) -> bool {
        let metrics_map = self.plugin_metrics.read().await;
        metrics_map.contains_key(&plugin_id)
    }

    /// Get the number of plugins being monitored.
    pub async fn monitored_plugin_count(&self) -> usize {
        let metrics_map = self.plugin_metrics.read().await;
        metrics_map.len()
    }
}

impl Default for PluginMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin-specific performance metrics.
///
/// Tracks comprehensive performance data for individual plugin instances
/// including invocation rates, latency percentiles, resource usage,
/// and custom metrics.
pub struct PluginMetrics {
    /// Total number of plugin invocations
    pub invocation_count: AtomicU64,

    /// Number of successful invocations
    pub successful_invocations: AtomicU64,

    /// Number of failed invocations
    pub failed_invocations: AtomicU64,

    /// Total processing time across all invocations
    pub total_processing_time: Arc<RwLock<Duration>>,

    /// Minimum processing time observed
    pub min_processing_time: Arc<RwLock<Option<Duration>>>,

    /// Maximum processing time observed
    pub max_processing_time: Arc<RwLock<Option<Duration>>>,

    /// Last invocation timestamp
    pub last_invocation: Arc<RwLock<Option<Instant>>>,

    /// Current memory usage (if available)
    pub current_memory_usage: AtomicU64,

    /// Peak memory usage observed
    pub peak_memory_usage: AtomicU64,

    /// Current CPU usage percentage
    pub current_cpu_usage: Arc<RwLock<f64>>,

    /// Peak CPU usage percentage
    pub peak_cpu_usage: Arc<RwLock<f64>>,

    /// Custom metrics defined by the plugin
    pub custom_metrics: Arc<RwLock<HashMap<String, MetricValue>>>,

    /// Metrics creation timestamp
    pub created_at: Instant,
}

impl Clone for PluginMetrics {
    fn clone(&self) -> Self {
        Self {
            invocation_count: AtomicU64::new(self.invocation_count.load(Ordering::Relaxed)),
            successful_invocations: AtomicU64::new(
                self.successful_invocations.load(Ordering::Relaxed),
            ),
            failed_invocations: AtomicU64::new(self.failed_invocations.load(Ordering::Relaxed)),
            total_processing_time: self.total_processing_time.clone(),
            min_processing_time: self.min_processing_time.clone(),
            max_processing_time: self.max_processing_time.clone(),
            last_invocation: self.last_invocation.clone(),
            current_memory_usage: AtomicU64::new(self.current_memory_usage.load(Ordering::Relaxed)),
            peak_memory_usage: AtomicU64::new(self.peak_memory_usage.load(Ordering::Relaxed)),
            current_cpu_usage: self.current_cpu_usage.clone(),
            peak_cpu_usage: self.peak_cpu_usage.clone(),
            custom_metrics: self.custom_metrics.clone(),
            created_at: self.created_at,
        }
    }
}

impl PluginMetrics {
    /// Create new plugin metrics.
    pub fn new() -> Self {
        Self {
            invocation_count: AtomicU64::new(0),
            successful_invocations: AtomicU64::new(0),
            failed_invocations: AtomicU64::new(0),
            total_processing_time: Arc::new(RwLock::new(Duration::default())),
            min_processing_time: Arc::new(RwLock::new(None)),
            max_processing_time: Arc::new(RwLock::new(None)),
            last_invocation: Arc::new(RwLock::new(None)),
            current_memory_usage: AtomicU64::new(0),
            peak_memory_usage: AtomicU64::new(0),
            current_cpu_usage: Arc::new(RwLock::new(0.0)),
            peak_cpu_usage: Arc::new(RwLock::new(0.0)),
            custom_metrics: Arc::new(RwLock::new(HashMap::new())),
            created_at: Instant::now(),
        }
    }

    /// Record a plugin invocation.
    pub async fn record_invocation(&self, duration: Duration, success: bool) {
        // Update atomic counters
        self.invocation_count.fetch_add(1, Ordering::Relaxed);
        if success {
            self.successful_invocations.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_invocations.fetch_add(1, Ordering::Relaxed);
        }

        // Update processing times
        {
            let mut total_time = self.total_processing_time.write().await;
            *total_time += duration;
        }

        // Update min time
        {
            let mut min_time = self.min_processing_time.write().await;
            match *min_time {
                None => *min_time = Some(duration),
                Some(current_min) if duration < current_min => *min_time = Some(duration),
                _ => {},
            }
        }

        // Update max time
        {
            let mut max_time = self.max_processing_time.write().await;
            match *max_time {
                None => *max_time = Some(duration),
                Some(current_max) if duration > current_max => *max_time = Some(duration),
                _ => {},
            }
        }

        // Update last invocation
        {
            let mut last = self.last_invocation.write().await;
            *last = Some(Instant::now());
        }
    }

    /// Get total number of invocations.
    pub fn total_invocations(&self) -> u64 {
        self.invocation_count.load(Ordering::Relaxed)
    }

    /// Get number of successful invocations.
    pub fn successful_invocations(&self) -> u64 {
        self.successful_invocations.load(Ordering::Relaxed)
    }

    /// Get number of failed invocations.
    pub fn failed_invocations(&self) -> u64 {
        self.failed_invocations.load(Ordering::Relaxed)
    }

    /// Get success rate as a percentage.
    pub fn success_rate(&self) -> f64 {
        let total = self.total_invocations();
        let successful = self.successful_invocations();

        if total > 0 {
            (successful as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Get failure rate as a percentage.
    pub fn failure_rate(&self) -> f64 {
        100.0 - self.success_rate()
    }

    /// Get average processing time.
    pub async fn average_processing_time(&self) -> Duration {
        let total = self.total_invocations();
        if total > 0 {
            let total_time = self.total_processing_time.read().await;
            *total_time / total as u32
        } else {
            Duration::default()
        }
    }

    /// Get minimum processing time.
    pub async fn min_processing_time(&self) -> Option<Duration> {
        let min_time = self.min_processing_time.read().await;
        *min_time
    }

    /// Get maximum processing time.
    pub async fn max_processing_time(&self) -> Option<Duration> {
        let max_time = self.max_processing_time.read().await;
        *max_time
    }

    /// Update memory usage.
    pub fn update_memory_usage(&self, current_bytes: u64) {
        self.current_memory_usage.store(current_bytes, Ordering::Relaxed);

        // Update peak if necessary
        loop {
            let current_peak = self.peak_memory_usage.load(Ordering::Relaxed);
            if current_bytes <= current_peak {
                break;
            }

            if self
                .peak_memory_usage
                .compare_exchange_weak(
                    current_peak,
                    current_bytes,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }
    }

    /// Update CPU usage.
    pub async fn update_cpu_usage(&self, percentage: f64) {
        {
            let mut current = self.current_cpu_usage.write().await;
            *current = percentage.clamp(0.0, 100.0);
        }

        {
            let mut peak = self.peak_cpu_usage.write().await;
            if percentage > *peak {
                *peak = percentage.clamp(0.0, 100.0);
            }
        }
    }

    /// Add or update a custom metric.
    pub async fn set_custom_metric(&self, name: String, value: MetricValue) {
        let mut metrics = self.custom_metrics.write().await;
        metrics.insert(name, value);
    }

    /// Get a custom metric value.
    pub async fn get_custom_metric(&self, name: &str) -> Option<MetricValue> {
        let metrics = self.custom_metrics.read().await;
        metrics.get(name).cloned()
    }

    /// Get all custom metrics.
    pub async fn get_all_custom_metrics(&self) -> HashMap<String, MetricValue> {
        let metrics = self.custom_metrics.read().await;
        metrics.clone()
    }

    /// Get metrics uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get current memory usage in bytes.
    pub fn current_memory(&self) -> u64 {
        self.current_memory_usage.load(Ordering::Relaxed)
    }

    /// Get peak memory usage in bytes.
    pub fn peak_memory(&self) -> u64 {
        self.peak_memory_usage.load(Ordering::Relaxed)
    }

    /// Get current CPU usage percentage.
    pub async fn current_cpu(&self) -> f64 {
        let current = self.current_cpu_usage.read().await;
        *current
    }

    /// Get peak CPU usage percentage.
    pub async fn peak_cpu(&self) -> f64 {
        let peak = self.peak_cpu_usage.read().await;
        *peak
    }

    /// Get last invocation timestamp.
    pub async fn last_invocation_time(&self) -> Option<Instant> {
        let last = self.last_invocation.read().await;
        *last
    }

    /// Convert to serializable snapshot.
    pub async fn to_snapshot(&self) -> PluginMetricsSnapshot {
        PluginMetricsSnapshot {
            invocation_count: self.total_invocations(),
            successful_invocations: self.successful_invocations(),
            failed_invocations: self.failed_invocations(),
            success_rate: self.success_rate(),
            average_processing_time: self.average_processing_time().await,
            min_processing_time: self.min_processing_time().await,
            max_processing_time: self.max_processing_time().await,
            current_memory_usage: self.current_memory(),
            peak_memory_usage: self.peak_memory(),
            current_cpu_usage: self.current_cpu().await,
            peak_cpu_usage: self.peak_cpu().await,
            custom_metrics: self.get_all_custom_metrics().await,
            uptime: self.uptime(),
            created_at_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

impl Default for PluginMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable snapshot of plugin metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetricsSnapshot {
    /// Total number of plugin invocations
    pub invocation_count: u64,

    /// Number of successful invocations
    pub successful_invocations: u64,

    /// Number of failed invocations
    pub failed_invocations: u64,

    /// Success rate as a percentage
    pub success_rate: f64,

    /// Average processing time in milliseconds
    pub average_processing_time: Duration,

    /// Minimum processing time observed
    pub min_processing_time: Option<Duration>,

    /// Maximum processing time observed
    pub max_processing_time: Option<Duration>,

    /// Current memory usage in bytes
    pub current_memory_usage: u64,

    /// Peak memory usage in bytes
    pub peak_memory_usage: u64,

    /// Current CPU usage percentage
    pub current_cpu_usage: f64,

    /// Peak CPU usage percentage
    pub peak_cpu_usage: f64,

    /// Custom metrics
    pub custom_metrics: HashMap<String, MetricValue>,

    /// Metrics uptime in milliseconds
    pub uptime: Duration,

    /// Creation timestamp (Unix timestamp)
    pub created_at_timestamp: u64,
}

/// Custom metric value types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    /// Integer counter value
    Counter(u64),

    /// Floating-point gauge value
    Gauge(f64),

    /// Duration measurement in milliseconds
    Duration(u64),

    /// Text value
    Text(String),

    /// Boolean flag
    Boolean(bool),

    /// Timestamp (Unix timestamp)
    Timestamp(u64),
}

impl MetricValue {
    /// Convert to u64 if possible.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Counter(value) => Some(*value),
            Self::Gauge(value) => Some(*value as u64),
            Self::Duration(value) => Some(*value),
            Self::Timestamp(value) => Some(*value),
            Self::Boolean(true) => Some(1),
            Self::Boolean(false) => Some(0),
            _ => None,
        }
    }

    /// Convert to f64 if possible.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Counter(value) => Some(*value as f64),
            Self::Gauge(value) => Some(*value),
            Self::Duration(value) => Some(*value as f64),
            Self::Timestamp(value) => Some(*value as f64),
            Self::Boolean(true) => Some(1.0),
            Self::Boolean(false) => Some(0.0),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_string(&self) -> String {
        match self {
            Self::Counter(value) => value.to_string(),
            Self::Gauge(value) => value.to_string(),
            Self::Duration(value) => format!("{}ms", value),
            Self::Text(value) => value.clone(),
            Self::Boolean(value) => value.to_string(),
            Self::Timestamp(value) => value.to_string(),
        }
    }
}

/// Global metrics aggregating data across all plugins.
#[derive(Debug)]
pub struct GlobalMetrics {
    /// Total invocations across all plugins
    total_invocations: AtomicU64,

    /// Total successful invocations
    total_successful: AtomicU64,

    /// Total failed invocations
    total_failed: AtomicU64,

    /// Global metrics creation timestamp
    created_at: Instant,
}

impl GlobalMetrics {
    /// Create new global metrics.
    pub fn new() -> Self {
        Self {
            total_invocations: AtomicU64::new(0),
            total_successful: AtomicU64::new(0),
            total_failed: AtomicU64::new(0),
            created_at: Instant::now(),
        }
    }

    /// Record a plugin invocation.
    pub fn record_invocation(&self, _duration: Duration, success: bool) {
        self.total_invocations.fetch_add(1, Ordering::Relaxed);
        if success {
            self.total_successful.fetch_add(1, Ordering::Relaxed);
        } else {
            self.total_failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get total invocations.
    pub fn total_invocations(&self) -> u64 {
        self.total_invocations.load(Ordering::Relaxed)
    }

    /// Get total successful invocations.
    pub fn total_successful(&self) -> u64 {
        self.total_successful.load(Ordering::Relaxed)
    }

    /// Get total failed invocations.
    pub fn total_failed(&self) -> u64 {
        self.total_failed.load(Ordering::Relaxed)
    }

    /// Get global success rate.
    pub fn global_success_rate(&self) -> f64 {
        let total = self.total_invocations();
        let successful = self.total_successful();

        if total > 0 {
            (successful as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Get global failure rate.
    pub fn global_failure_rate(&self) -> f64 {
        100.0 - self.global_success_rate()
    }

    /// Get uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Reset all metrics.
    pub fn reset(&self) {
        self.total_invocations.store(0, Ordering::Relaxed);
        self.total_successful.store(0, Ordering::Relaxed);
        self.total_failed.store(0, Ordering::Relaxed);
    }
}

impl Default for GlobalMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitor configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Enable metrics collection
    pub enabled: bool,

    /// Metrics collection interval in milliseconds
    pub collection_interval_ms: u64,

    /// Maximum number of plugins to monitor
    pub max_plugins: Option<usize>,

    /// Enable memory usage tracking
    pub track_memory: bool,

    /// Enable CPU usage tracking
    pub track_cpu: bool,

    /// Enable custom metrics
    pub enable_custom_metrics: bool,

    /// Maximum number of custom metrics per plugin
    pub max_custom_metrics: usize,

    /// Metrics retention duration in seconds
    pub retention_duration_secs: u64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval_ms: 1000, // 1 second
            max_plugins: Some(1000),
            track_memory: true,
            track_cpu: true,
            enable_custom_metrics: true,
            max_custom_metrics: 100,
            retention_duration_secs: 3600, // 1 hour
        }
    }
}

impl MonitorConfig {
    /// Create config for high-performance scenarios.
    pub fn high_performance() -> Self {
        Self {
            enabled: true,
            collection_interval_ms: 100,
            max_plugins: Some(100),
            track_memory: false,
            track_cpu: false,
            enable_custom_metrics: false,
            max_custom_metrics: 10,
            retention_duration_secs: 300, // 5 minutes
        }
    }

    /// Create config for development environments.
    pub fn development() -> Self {
        Self {
            enabled: true,
            collection_interval_ms: 500,
            max_plugins: None,
            track_memory: true,
            track_cpu: true,
            enable_custom_metrics: true,
            max_custom_metrics: 1000,
            retention_duration_secs: 7200, // 2 hours
        }
    }

    /// Create minimal monitoring config.
    pub fn minimal() -> Self {
        Self {
            enabled: true,
            collection_interval_ms: 10000, // 10 seconds
            max_plugins: Some(50),
            track_memory: false,
            track_cpu: false,
            enable_custom_metrics: false,
            max_custom_metrics: 0,
            retention_duration_secs: 600, // 10 minutes
        }
    }

    /// Get collection interval as Duration.
    pub fn collection_interval(&self) -> Duration {
        Duration::from_millis(self.collection_interval_ms)
    }

    /// Get retention duration as Duration.
    pub fn retention_duration(&self) -> Duration {
        Duration::from_secs(self.retention_duration_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metrics_creation() {
        let metrics = PluginMetrics::new();
        assert_eq!(metrics.total_invocations(), 0);
        assert_eq!(metrics.successful_invocations(), 0);
        assert_eq!(metrics.failed_invocations(), 0);
        assert_eq!(metrics.success_rate(), 0.0);
        assert_eq!(metrics.current_memory(), 0);
        assert_eq!(metrics.peak_memory(), 0);
    }

    #[tokio::test]
    async fn test_plugin_metrics_invocation_recording() {
        let metrics = PluginMetrics::new();

        // Record successful invocation
        metrics.record_invocation(Duration::from_millis(100), true).await;
        assert_eq!(metrics.total_invocations(), 1);
        assert_eq!(metrics.successful_invocations(), 1);
        assert_eq!(metrics.failed_invocations(), 0);
        assert_eq!(metrics.success_rate(), 100.0);

        // Record failed invocation
        metrics.record_invocation(Duration::from_millis(200), false).await;
        assert_eq!(metrics.total_invocations(), 2);
        assert_eq!(metrics.successful_invocations(), 1);
        assert_eq!(metrics.failed_invocations(), 1);
        assert_eq!(metrics.success_rate(), 50.0);

        // Check timing metrics
        let avg_time = metrics.average_processing_time().await;
        assert_eq!(avg_time, Duration::from_millis(150));

        let min_time = metrics.min_processing_time().await;
        assert_eq!(min_time, Some(Duration::from_millis(100)));

        let max_time = metrics.max_processing_time().await;
        assert_eq!(max_time, Some(Duration::from_millis(200)));
    }

    #[tokio::test]
    async fn test_custom_metrics() {
        let metrics = PluginMetrics::new();

        // Set custom metrics
        metrics
            .set_custom_metric("requests_per_second".to_string(), MetricValue::Gauge(42.5))
            .await;
        metrics
            .set_custom_metric("cache_hits".to_string(), MetricValue::Counter(1000))
            .await;
        metrics
            .set_custom_metric("feature_enabled".to_string(), MetricValue::Boolean(true))
            .await;

        // Retrieve custom metrics
        let rps = metrics.get_custom_metric("requests_per_second").await;
        assert!(matches!(rps, Some(MetricValue::Gauge(42.5))));

        let cache_hits = metrics.get_custom_metric("cache_hits").await;
        assert!(matches!(cache_hits, Some(MetricValue::Counter(1000))));

        let feature = metrics.get_custom_metric("feature_enabled").await;
        assert!(matches!(feature, Some(MetricValue::Boolean(true))));

        // Get all metrics
        let all_metrics = metrics.get_all_custom_metrics().await;
        assert_eq!(all_metrics.len(), 3);
    }

    #[test]
    fn test_memory_usage_tracking() {
        let metrics = PluginMetrics::new();

        // Update memory usage
        metrics.update_memory_usage(1024);
        assert_eq!(metrics.current_memory(), 1024);
        assert_eq!(metrics.peak_memory(), 1024);

        // Update to higher usage
        metrics.update_memory_usage(2048);
        assert_eq!(metrics.current_memory(), 2048);
        assert_eq!(metrics.peak_memory(), 2048);

        // Update to lower usage (peak should remain)
        metrics.update_memory_usage(512);
        assert_eq!(metrics.current_memory(), 512);
        assert_eq!(metrics.peak_memory(), 2048);
    }

    #[tokio::test]
    async fn test_cpu_usage_tracking() {
        let metrics = PluginMetrics::new();

        // Update CPU usage
        metrics.update_cpu_usage(25.5).await;
        assert_eq!(metrics.current_cpu().await, 25.5);
        assert_eq!(metrics.peak_cpu().await, 25.5);

        // Update to higher usage
        metrics.update_cpu_usage(75.0).await;
        assert_eq!(metrics.current_cpu().await, 75.0);
        assert_eq!(metrics.peak_cpu().await, 75.0);

        // Update to lower usage (peak should remain)
        metrics.update_cpu_usage(10.0).await;
        assert_eq!(metrics.current_cpu().await, 10.0);
        assert_eq!(metrics.peak_cpu().await, 75.0);

        // Test clamping
        metrics.update_cpu_usage(150.0).await;
        assert_eq!(metrics.current_cpu().await, 100.0);
        assert_eq!(metrics.peak_cpu().await, 100.0);
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let metrics = PluginMetrics::new();

        // Record some data
        metrics.record_invocation(Duration::from_millis(100), true).await;
        metrics.record_invocation(Duration::from_millis(200), false).await;
        metrics.update_memory_usage(1024);
        metrics.update_cpu_usage(50.0).await;
        metrics.set_custom_metric("test".to_string(), MetricValue::Counter(42)).await;

        // Create snapshot
        let snapshot = metrics.to_snapshot().await;
        assert_eq!(snapshot.invocation_count, 2);
        assert_eq!(snapshot.successful_invocations, 1);
        assert_eq!(snapshot.failed_invocations, 1);
        assert_eq!(snapshot.success_rate, 50.0);
        assert_eq!(snapshot.current_memory_usage, 1024);
        assert_eq!(snapshot.peak_memory_usage, 1024);
        assert_eq!(snapshot.current_cpu_usage, 50.0);
        assert_eq!(snapshot.peak_cpu_usage, 50.0);
        assert_eq!(snapshot.custom_metrics.len(), 1);
    }

    #[test]
    fn test_metric_value_conversions() {
        let counter = MetricValue::Counter(42);
        assert_eq!(counter.as_u64(), Some(42));
        assert_eq!(counter.as_f64(), Some(42.0));

        let gauge = MetricValue::Gauge(3.14);
        assert_eq!(gauge.as_u64(), Some(3));
        assert_eq!(gauge.as_f64(), Some(3.14));

        let bool_true = MetricValue::Boolean(true);
        assert_eq!(bool_true.as_u64(), Some(1));
        assert_eq!(bool_true.as_f64(), Some(1.0));

        let bool_false = MetricValue::Boolean(false);
        assert_eq!(bool_false.as_u64(), Some(0));
        assert_eq!(bool_false.as_f64(), Some(0.0));

        let text = MetricValue::Text("hello".to_string());
        assert_eq!(text.as_u64(), None);
        assert_eq!(text.as_f64(), None);
        assert_eq!(text.as_string(), "hello");
    }

    #[test]
    fn test_global_metrics() {
        let global = GlobalMetrics::new();

        // Record some invocations
        global.record_invocation(Duration::from_millis(100), true);
        global.record_invocation(Duration::from_millis(200), false);
        global.record_invocation(Duration::from_millis(150), true);

        assert_eq!(global.total_invocations(), 3);
        assert_eq!(global.total_successful(), 2);
        assert_eq!(global.total_failed(), 1);
        assert_eq!(global.global_success_rate(), 2.0 / 3.0 * 100.0);

        // Reset
        global.reset();
        assert_eq!(global.total_invocations(), 0);
        assert_eq!(global.total_successful(), 0);
        assert_eq!(global.total_failed(), 0);
    }

    #[test]
    fn test_monitor_config() {
        let default_config = MonitorConfig::default();
        assert!(default_config.enabled);
        assert_eq!(default_config.collection_interval(), Duration::from_secs(1));
        assert!(default_config.track_memory);
        assert!(default_config.track_cpu);

        let perf_config = MonitorConfig::high_performance();
        assert!(perf_config.enabled);
        assert_eq!(perf_config.collection_interval(), Duration::from_millis(100));
        assert!(!perf_config.track_memory);
        assert!(!perf_config.track_cpu);

        let minimal_config = MonitorConfig::minimal();
        assert!(minimal_config.enabled);
        assert_eq!(minimal_config.collection_interval(), Duration::from_secs(10));
        assert!(!minimal_config.track_memory);
        assert!(!minimal_config.track_cpu);
        assert!(!minimal_config.enable_custom_metrics);
    }

    #[tokio::test]
    async fn test_plugin_monitor() {
        use crate::plugin::PluginId;

        let monitor = PluginMonitor::new();
        let plugin_id = PluginId::new();

        // Track an invocation
        let result = monitor.track_invocation(plugin_id, async { 42 }).await;
        assert_eq!(result, 42);

        // Check metrics were recorded
        assert!(monitor.is_monitoring(plugin_id).await);
        assert_eq!(monitor.monitored_plugin_count().await, 1);

        let metrics = monitor.collect_metrics(plugin_id).await.unwrap();
        assert_eq!(metrics.invocation_count, 1);
        assert_eq!(metrics.successful_invocations, 1);
        assert_eq!(metrics.success_rate, 100.0);

        // Clear metrics
        monitor.clear_plugin_metrics(plugin_id).await;
        assert!(!monitor.is_monitoring(plugin_id).await);
        assert_eq!(monitor.monitored_plugin_count().await, 0);
    }
}
