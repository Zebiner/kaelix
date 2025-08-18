//! # Telemetry & Observability Framework
//!
//! Ultra-high-performance telemetry framework designed for <1% system overhead
//! while providing complete observability across MemoryStreamer components.
//!
//! ## Performance Targets
//! - **Metric recording**: <100ns per operation
//! - **Total system overhead**: <1% of system performance  
//! - **Memory overhead**: <10MB for telemetry system
//! - **Export latency**: <1ms for metric collection
//! - **Log throughput**: >1M log entries/second
//!
//! ## Features
//! - Zero-allocation hot path metrics collection
//! - SIMD-optimized histogram operations
//! - Distributed tracing with minimal overhead
//! - High-performance structured logging
//! - OpenTelemetry integration
//! - Multi-format metrics export (Prometheus, InfluxDB, etc.)
//!
//! ## Usage
//!
//! ```rust
//! use kaelix_core::telemetry::*;
//!
//! // Initialize telemetry system
//! let config = TelemetryConfig::default();
//! let telemetry = TelemetrySystem::new(config).await?;
//!
//! // Record metrics (zero-allocation hot path)
//! telemetry.metrics().increment_counter(MetricKey::MessagesProcessed);
//! telemetry.metrics().record_latency(MetricKey::ProcessingLatency, 5000); // 5μs
//!
//! // Distributed tracing
//! let span = telemetry.tracing().start_span("message_processing", None);
//! // ... processing work ...
//! telemetry.tracing().end_span(span);
//!
//! // Structured logging
//! telemetry.logging().info("message_processor", "Message processed successfully", &[
//!     LogField::new("message_id", "12345"),
//!     LogField::new("latency_us", "4.2"),
//! ]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod collector;
pub mod config;
pub mod error;
pub mod exporter;
pub mod logging;
pub mod metrics;
pub mod registry;
pub mod tracing;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

pub use self::collector::*;
pub use self::config::*;
pub use self::error::*;
pub use self::exporter::*;
pub use self::logging::*;
pub use self::metrics::*;
pub use self::registry::*;
pub use self::tracing::*;

/// Main telemetry system that coordinates all observability components
#[derive(Debug, Clone)]
pub struct TelemetrySystem {
    /// High-performance metrics collector
    metrics: Arc<MetricsCollector>,
    /// Distributed tracing system
    tracing: Arc<TracingSystem>,
    /// Structured logging system
    logging: Arc<LoggingSystem>,
    /// Metrics registry and management
    registry: Arc<MetricsRegistry>,
    /// Metrics export infrastructure
    exporter: Arc<MetricsExporter>,
    /// Telemetry configuration
    config: TelemetryConfig,
    /// System start time for uptime metrics
    start_time: Instant,
}

impl TelemetrySystem {
    /// Creates a new telemetry system with the given configuration
    ///
    /// # Performance
    /// Initialization is optimized for minimal startup overhead with pre-allocated
    /// data structures and SIMD-optimized histogram buckets.
    ///
    /// # Errors
    /// Returns `TelemetryError` if system initialization fails
    pub async fn new(config: TelemetryConfig) -> Result<Self, TelemetryError> {
        let start_time = Instant::now();

        // Initialize metrics registry first
        let mut registry = MetricsRegistry::new(config.metrics.clone())?;
        
        // Register core MemoryStreamer metrics
        Self::register_core_metrics(&mut registry)?;
        
        let registry = Arc::new(registry);

        // Initialize metrics collector with zero-allocation design
        let metrics = Arc::new(MetricsCollector::new(
            config.metrics.clone(),
            Arc::clone(&registry),
        )?);

        // Initialize distributed tracing system
        let tracing = Arc::new(TracingSystem::new(config.tracing.clone()).await?);

        // Initialize high-performance logging system
        let logging = Arc::new(LoggingSystem::new(config.logging.clone()).await?);

        // Initialize metrics exporter
        let exporter = Arc::new(MetricsExporter::new(
            config.export.clone(),
            Arc::clone(&registry),
        ).await?);

        Ok(Self {
            metrics,
            tracing,
            logging,
            registry,
            exporter,
            config,
            start_time,
        })
    }

    /// Gets the metrics collector for high-performance metric recording
    #[inline(always)]
    pub fn metrics(&self) -> &MetricsCollector {
        &self.metrics
    }

    /// Gets the distributed tracing system
    #[inline(always)]
    pub fn tracing(&self) -> &TracingSystem {
        &self.tracing
    }

    /// Gets the structured logging system
    #[inline(always)]
    pub fn logging(&self) -> &LoggingSystem {
        &self.logging
    }

    /// Gets the metrics registry
    #[inline(always)]
    pub fn registry(&self) -> &MetricsRegistry {
        &self.registry
    }

    /// Gets the metrics exporter
    #[inline(always)]
    pub fn exporter(&self) -> &MetricsExporter {
        &self.exporter
    }

    /// Returns the telemetry configuration
    #[inline(always)]
    pub fn config(&self) -> &TelemetryConfig {
        &self.config
    }

    /// Returns system uptime since telemetry initialization
    #[inline(always)]
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Starts the telemetry system background services
    ///
    /// This starts:
    /// - Metrics collection and aggregation
    /// - Trace span processing and export
    /// - Log entry processing and writing
    /// - Scheduled metrics export
    ///
    /// # Errors
    /// Returns `TelemetryError` if any background service fails to start
    pub async fn start(&mut self) -> Result<(), TelemetryError> {
        // Start logging system background writer
        self.logging.start_background_writer().await?;

        // Start tracing system span processor
        self.tracing.start_span_processor().await?;

        // Start metrics exporter scheduler
        self.exporter.start_scheduled_export().await?;

        // Record system startup
        self.metrics.increment_counter(MetricKey::SystemStartups);
        self.metrics.set_gauge(MetricKey::SystemUptime, 0.0);

        Ok(())
    }

    /// Gracefully shuts down the telemetry system
    ///
    /// Ensures all pending metrics are exported, traces are processed,
    /// and log entries are written before shutdown.
    ///
    /// # Performance
    /// Shutdown is optimized to complete within 1 second for graceful service termination.
    pub async fn shutdown(&mut self) -> Result<(), TelemetryError> {
        // Stop scheduled exports first
        self.exporter.stop_export().await?;

        // Flush all pending data
        self.flush_all_pending().await?;

        // Stop background services
        self.tracing.stop_span_processor().await?;
        self.logging.stop_background_writer().await?;

        Ok(())
    }

    /// Flushes all pending telemetry data
    ///
    /// Forces immediate processing of:
    /// - Pending log entries
    /// - Buffered trace spans
    /// - Cached metrics for export
    ///
    /// # Performance
    /// Flush operations complete within 100ms target
    pub async fn flush_all_pending(&self) -> Result<(), TelemetryError> {
        // Flush logging system
        self.logging.flush().await?;

        // Flush tracing system
        self.tracing.flush_spans().await?;

        // Export current metrics snapshot
        let snapshot = self.metrics.collect_metrics();
        self.exporter.export_snapshot(snapshot).await?;

        Ok(())
    }

    /// Registers core MemoryStreamer metrics with the registry
    fn register_core_metrics(registry: &mut MetricsRegistry) -> Result<(), TelemetryError> {
        // System metrics
        registry.register_metric(MetricDefinition {
            key: MetricKey::SystemStartups,
            name: "system_startups_total".to_string(),
            description: "Total number of system startups".to_string(),
            metric_type: MetricType::Counter,
            labels: vec![],
            unit: None,
        })?;

        registry.register_metric(MetricDefinition {
            key: MetricKey::SystemUptime,
            name: "system_uptime_seconds".to_string(),
            description: "System uptime in seconds".to_string(),
            metric_type: MetricType::Gauge,
            labels: vec![],
            unit: Some("seconds".to_string()),
        })?;

        // Message processing metrics
        registry.register_metric(MetricDefinition {
            key: MetricKey::MessagesProcessed,
            name: "messages_processed_total".to_string(),
            description: "Total number of messages processed".to_string(),
            metric_type: MetricType::Counter,
            labels: vec!["component".to_string()],
            unit: None,
        })?;

        registry.register_metric(MetricDefinition {
            key: MetricKey::ProcessingLatency,
            name: "processing_latency_microseconds".to_string(),
            description: "Message processing latency distribution".to_string(),
            metric_type: MetricType::Histogram,
            labels: vec!["component".to_string()],
            unit: Some("microseconds".to_string()),
        })?;

        // Network metrics
        registry.register_metric(MetricDefinition {
            key: MetricKey::ActiveConnections,
            name: "active_connections".to_string(),
            description: "Number of active network connections".to_string(),
            metric_type: MetricType::Gauge,
            labels: vec!["protocol".to_string()],
            unit: None,
        })?;

        registry.register_metric(MetricDefinition {
            key: MetricKey::BytesReceived,
            name: "bytes_received_total".to_string(),
            description: "Total bytes received from network".to_string(),
            metric_type: MetricType::Counter,
            labels: vec!["protocol".to_string()],
            unit: Some("bytes".to_string()),
        })?;

        registry.register_metric(MetricDefinition {
            key: MetricKey::BytesSent,
            name: "bytes_sent_total".to_string(),
            description: "Total bytes sent to network".to_string(),
            metric_type: MetricType::Counter,
            labels: vec!["protocol".to_string()],
            unit: Some("bytes".to_string()),
        })?;

        // Plugin system metrics
        registry.register_metric(MetricDefinition {
            key: MetricKey::PluginsLoaded,
            name: "plugins_loaded".to_string(),
            description: "Number of loaded plugins".to_string(),
            metric_type: MetricType::Gauge,
            labels: vec![],
            unit: None,
        })?;

        registry.register_metric(MetricDefinition {
            key: MetricKey::PluginInvocations,
            name: "plugin_invocations_total".to_string(),
            description: "Total plugin invocations".to_string(),
            metric_type: MetricType::Counter,
            labels: vec!["plugin".to_string(), "method".to_string()],
            unit: None,
        })?;

        Ok(())
    }

    /// Updates the system uptime gauge (called periodically)
    pub fn update_uptime_metric(&self) {
        let uptime_seconds = self.uptime().as_secs_f64();
        self.metrics.set_gauge(MetricKey::SystemUptime, uptime_seconds);
    }

    /// Collects comprehensive system health metrics
    pub fn collect_health_metrics(&self) -> HealthMetrics {
        HealthMetrics {
            uptime: self.uptime(),
            metrics_collected: self.metrics.total_metrics_collected(),
            traces_active: self.tracing.active_traces_count(),
            logs_processed: self.logging.total_logs_processed(),
            export_success_rate: self.exporter.success_rate(),
            memory_usage_bytes: self.estimate_memory_usage(),
            cpu_usage_percent: self.estimate_cpu_usage(),
        }
    }

    /// Estimates current telemetry system memory usage
    fn estimate_memory_usage(&self) -> usize {
        // Rough estimation of memory usage across all components
        self.metrics.memory_usage() +
        self.tracing.memory_usage() +
        self.logging.memory_usage() +
        self.registry.memory_usage() +
        self.exporter.memory_usage()
    }

    /// Estimates current telemetry system CPU usage
    fn estimate_cpu_usage(&self) -> f64 {
        // CPU usage estimation based on component activity
        let metrics_cpu = self.metrics.cpu_usage_estimate();
        let tracing_cpu = self.tracing.cpu_usage_estimate();
        let logging_cpu = self.logging.cpu_usage_estimate();
        let export_cpu = self.exporter.cpu_usage_estimate();
        
        metrics_cpu + tracing_cpu + logging_cpu + export_cpu
    }
}

/// System health metrics collected by telemetry
#[derive(Debug, Clone)]
pub struct HealthMetrics {
    /// System uptime
    pub uptime: Duration,
    /// Total metrics collected
    pub metrics_collected: u64,
    /// Number of active traces
    pub traces_active: usize,
    /// Total log entries processed
    pub logs_processed: u64,
    /// Export success rate (0.0-1.0)
    pub export_success_rate: f64,
    /// Estimated memory usage in bytes
    pub memory_usage_bytes: usize,
    /// Estimated CPU usage percentage
    pub cpu_usage_percent: f64,
}

/// Core metric keys used throughout MemoryStreamer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum MetricKey {
    // System metrics
    SystemStartups = 0,
    SystemUptime = 1,
    
    // Message processing metrics
    MessagesProcessed = 100,
    ProcessingLatency = 101,
    MessageErrors = 102,
    
    // Network metrics
    ActiveConnections = 200,
    BytesReceived = 201,
    BytesSent = 202,
    NetworkErrors = 203,
    ConnectionLatency = 204,
    
    // Plugin system metrics
    PluginsLoaded = 300,
    PluginInvocations = 301,
    PluginErrors = 302,
    PluginLoadTime = 303,
    
    // Runtime metrics
    ThreadsActive = 400,
    TasksQueued = 401,
    MemoryUsage = 402,
    CpuUsage = 403,
}

impl MetricKey {
    /// Returns the metric key as a u32 for efficient hashing
    #[inline(always)]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Returns the string representation of the metric key
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SystemStartups => "system.startups",
            Self::SystemUptime => "system.uptime",
            Self::MessagesProcessed => "messages.processed",
            Self::ProcessingLatency => "messages.processing_latency",
            Self::MessageErrors => "messages.errors",
            Self::ActiveConnections => "network.active_connections",
            Self::BytesReceived => "network.bytes_received",
            Self::BytesSent => "network.bytes_sent",
            Self::NetworkErrors => "network.errors",
            Self::ConnectionLatency => "network.connection_latency",
            Self::PluginsLoaded => "plugins.loaded",
            Self::PluginInvocations => "plugins.invocations",
            Self::PluginErrors => "plugins.errors",
            Self::PluginLoadTime => "plugins.load_time",
            Self::ThreadsActive => "runtime.threads_active",
            Self::TasksQueued => "runtime.tasks_queued",
            Self::MemoryUsage => "runtime.memory_usage",
            Self::CpuUsage => "runtime.cpu_usage",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_telemetry_system_creation() {
        let config = TelemetryConfig::default();
        let telemetry = TelemetrySystem::new(config).await.unwrap();
        
        assert!(telemetry.uptime().as_nanos() > 0);
        assert!(telemetry.config().enabled);
    }

    #[test]
    fn test_metric_key_conversion() {
        assert_eq!(MetricKey::SystemStartups.as_u32(), 0);
        assert_eq!(MetricKey::MessagesProcessed.as_u32(), 100);
        assert_eq!(MetricKey::ActiveConnections.as_u32(), 200);
        assert_eq!(MetricKey::PluginsLoaded.as_u32(), 300);
        assert_eq!(MetricKey::ThreadsActive.as_u32(), 400);
    }

    #[test]
    fn test_metric_key_string_repr() {
        assert_eq!(MetricKey::SystemStartups.as_str(), "system.startups");
        assert_eq!(MetricKey::ProcessingLatency.as_str(), "messages.processing_latency");
        assert_eq!(MetricKey::NetworkErrors.as_str(), "network.errors");
    }

    #[bench]
    fn bench_metric_key_hash(b: &mut test::Bencher) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            MetricKey::MessagesProcessed.hash(&mut hasher);
            hasher.finish()
        });
    }
}