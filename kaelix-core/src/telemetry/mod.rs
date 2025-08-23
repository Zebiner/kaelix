//! Kaelix Telemetry System
//!
//! Comprehensive telemetry infrastructure for metrics collection, logging,
//! distributed tracing, and performance monitoring with OpenTelemetry compatibility.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

pub mod collector;
pub mod config;
pub mod error;
pub mod exporter;
pub mod logging;
pub mod metrics;
pub mod performance;
pub mod registry;
pub mod tracing;

// Re-exports
pub use collector::MetricsCollector;
pub use config::*;
pub use error::TelemetryError;
pub use exporter::MetricsExporter;
pub use performance::PerformanceCounters;
pub use registry::{MetricDefinition, MetricFamily, MetricType, MetricsRegistry};
pub use tracing::TracingSystem;

/// Result type for telemetry operations
pub type Result<T> = std::result::Result<T, TelemetryError>;

/// Unique identifier for a metric
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricKey {
    pub name: String,
    pub labels: HashMap<String, String>,
}

impl MetricKey {
    pub fn new(name: String) -> Self {
        Self { name, labels: HashMap::new() }
    }

    pub fn with_label(mut self, key: String, value: String) -> Self {
        self.labels.insert(key, value);
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for MetricKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.labels.is_empty() {
            write!(f, "{{")?;
            let mut first = true;
            for (k, v) in &self.labels {
                if !first {
                    write!(f, ",")?;
                }
                write!(f, "{}=\"{}\"", k, v)?;
                first = false;
            }
            write!(f, "}}")?;
        }
        Ok(())
    }
}

/// Histogram bucket data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramData {
    pub buckets: Vec<(f64, u64)>, // (upper_bound, count)
    pub count: u64,
    pub sum: f64,
}

/// Summary quantile data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryData {
    pub quantiles: Vec<QuantileEntry>,
    pub count: u64,
    pub sum: f64,
}

/// Single quantile entry in summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantileEntry {
    pub quantile: f64,
    pub value: f64,
}

/// Snapshot of all metrics at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: SystemTime,
    pub metrics: HashMap<MetricKey, metrics::AggregatedData>,
}

/// Trait for metrics registry implementations
pub trait MetricsRegistryTrait: Send + Sync {
    fn get_metric_definition(&self, key: MetricKey) -> Option<Arc<registry::MetricDefinition>>;

    fn list_metrics(&self) -> Vec<MetricKey>;

    fn search_metrics(&self, pattern: &str) -> Vec<MetricKey> {
        self.list_metrics()
            .into_iter()
            .filter(|key| key.name.contains(pattern))
            .collect()
    }

    fn get_metrics_by_type(&self, _metric_type: registry::MetricType) -> Vec<MetricKey> {
        // Default implementation - registries can override for efficiency
        Vec::new()
    }
}

/// Telemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub metrics: MetricsConfig,
    pub logging: LoggingConfig,
    pub tracing: TracingConfig,
    pub performance: PerformanceConfig,
    pub enabled: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            metrics: MetricsConfig::default(),
            logging: LoggingConfig::default(),
            tracing: TracingConfig::default(),
            performance: PerformanceConfig::default(),
            enabled: true,
        }
    }
}

/// Performance monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub cpu_monitoring: bool,
    pub memory_monitoring: bool,
    pub disk_monitoring: bool,
    pub network_monitoring: bool,
    pub collection_interval: std::time::Duration,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            cpu_monitoring: true,
            memory_monitoring: true,
            disk_monitoring: false,
            network_monitoring: false,
            collection_interval: std::time::Duration::from_secs(60),
        }
    }
}

/// Global telemetry configuration
pub static mut TELEMETRY_CONFIG: Option<TelemetryConfig> = None;

/// Initialize global telemetry system
pub fn init_telemetry(config: TelemetryConfig) -> Result<()> {
    unsafe {
        TELEMETRY_CONFIG = Some(config);
    }
    Ok(())
}

/// Get global telemetry configuration
pub fn get_config() -> Option<&'static TelemetryConfig> {
    unsafe { TELEMETRY_CONFIG.as_ref() }
}

/// Check if telemetry is enabled
pub fn is_enabled() -> bool {
    get_config().map(|c| c.enabled).unwrap_or(false)
}

/// Record a counter metric
pub fn counter(key: &str, _value: f64, labels: HashMap<String, String>) -> Result<()> {
    if !is_enabled() {
        return Ok(());
    }

    let _metric_key = MetricKey { name: key.to_string(), labels };

    // TODO: Use global collector instance
    Ok(())
}

/// Record a gauge metric
pub fn gauge(key: &str, _value: f64, labels: HashMap<String, String>) -> Result<()> {
    if !is_enabled() {
        return Ok(());
    }

    let _metric_key = MetricKey { name: key.to_string(), labels };

    // TODO: Use global collector instance
    Ok(())
}

/// Record a histogram metric
pub fn histogram(key: &str, _value: f64, labels: HashMap<String, String>) -> Result<()> {
    if !is_enabled() {
        return Ok(());
    }

    let _metric_key = MetricKey { name: key.to_string(), labels };

    // TODO: Use global collector instance
    Ok(())
}

/// Simple in-memory metrics registry for basic use cases
#[derive(Debug)]
pub struct InMemoryMetricsRegistry {
    metrics: std::sync::Mutex<HashMap<String, registry::MetricType>>,
}

impl Default for InMemoryMetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryMetricsRegistry {
    pub fn new() -> Self {
        Self { metrics: std::sync::Mutex::new(std::collections::HashMap::new()) }
    }
}

impl MetricsRegistryTrait for InMemoryMetricsRegistry {
    fn get_metric_definition(&self, _key: MetricKey) -> Option<Arc<registry::MetricDefinition>> {
        None // Simple implementation - no definitions stored
    }

    fn list_metrics(&self) -> Vec<MetricKey> {
        Vec::new() // Simple implementation
    }

    fn search_metrics(&self, _pattern: &str) -> Vec<MetricKey> {
        Vec::new()
    }

    fn get_metrics_by_type(&self, _metric_type: registry::MetricType) -> Vec<MetricKey> {
        Vec::new()
    }
}

/// Telemetry subsystem for collecting and exporting observability data
#[derive(Debug)]
pub struct TelemetrySubsystem {
    /// Metrics collector
    collector: MetricsCollector,
    /// Metrics exporter
    exporter: MetricsExporter,
    /// Performance monitor
    performance: PerformanceCounters,
    /// Configuration
    config: TelemetryConfig,
}

impl TelemetrySubsystem {
    /// Create a new telemetry subsystem
    pub fn new(config: TelemetryConfig) -> Result<Self> {
        let collector = MetricsCollector::new(config.metrics.clone());
        let exporter = MetricsExporter::new(config.metrics.exporter.clone())?;
        let performance = PerformanceCounters::new();

        Ok(Self { collector, exporter, performance, config })
    }

    /// Start all telemetry subsystems
    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Start performance monitoring
        self.performance.start().await?;

        // Start metrics exporter
        self.exporter.start().await?;

        Ok(())
    }

    /// Stop all telemetry subsystems
    pub async fn stop(&self) -> Result<()> {
        self.exporter.stop().await?;
        self.performance.stop().await?;
        Ok(())
    }

    /// Get metrics snapshot
    pub fn snapshot(&self) -> Result<MetricsSnapshot> {
        self.collector.snapshot()
    }

    /// Record a metric value
    pub fn record(&self, key: &MetricKey, value: f64) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.collector.record(key, value)
    }

    /// Get collector reference
    pub fn collector(&self) -> &MetricsCollector {
        &self.collector
    }

    /// Get exporter reference
    pub fn exporter(&self) -> &MetricsExporter {
        &self.exporter
    }

    /// Get performance monitor reference
    pub fn performance(&self) -> &PerformanceCounters {
        &self.performance
    }
}

/// Telemetry builder for constructing telemetry systems
pub struct TelemetryBuilder {
    config: TelemetryConfig,
}

impl TelemetryBuilder {
    /// Create a new telemetry builder
    pub fn new() -> Self {
        Self { config: TelemetryConfig::default() }
    }

    /// Enable/disable telemetry
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    /// Set metrics configuration
    pub fn with_metrics(mut self, config: MetricsConfig) -> Self {
        self.config.metrics = config;
        self
    }

    /// Set logging configuration
    pub fn with_logging(mut self, config: LoggingConfig) -> Self {
        self.config.logging = config;
        self
    }

    /// Set tracing configuration
    pub fn with_tracing(mut self, config: TracingConfig) -> Self {
        self.config.tracing = config;
        self
    }

    /// Set performance configuration
    pub fn with_performance(mut self, config: PerformanceConfig) -> Self {
        self.config.performance = config;
        self
    }

    /// Build the telemetry subsystem
    pub fn build(self) -> Result<TelemetrySubsystem> {
        TelemetrySubsystem::new(self.config)
    }
}

impl Default for TelemetryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_key_creation() {
        let key = MetricKey::new("test_metric".to_string())
            .with_label("service".to_string(), "api".to_string())
            .with_label("env".to_string(), "prod".to_string());

        assert_eq!(key.name, "test_metric");
        assert_eq!(key.labels.len(), 2);
        assert_eq!(key.labels.get("service"), Some(&"api".to_string()));
        assert_eq!(key.labels.get("env"), Some(&"prod".to_string()));
    }

    #[test]
    fn test_metric_key_display() {
        let key = MetricKey::new("test_metric".to_string())
            .with_label("service".to_string(), "api".to_string());

        let display = format!("{}", key);
        assert!(display.contains("test_metric"));
        assert!(display.contains("service=\"api\""));
    }

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert!(config.enabled);
    }

    #[test]
    fn test_telemetry_builder() {
        let subsystem = TelemetryBuilder::new()
            .enabled(true)
            .with_metrics(MetricsConfig::default())
            .build();

        assert!(subsystem.is_ok());
    }
}
