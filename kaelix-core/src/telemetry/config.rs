//! # Telemetry Configuration
//!
//! Comprehensive configuration system for the telemetry & observability framework.
//! Designed for zero-allocation validation and high-performance operation.

use std::collections::HashMap;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};
use crate::telemetry::{TelemetryError, Result};

/// Main telemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TelemetryConfig {
    /// Whether telemetry is enabled
    pub enabled: bool,

    /// Metrics collection configuration
    #[validate(nested)]
    pub metrics: MetricsConfig,

    /// Distributed tracing configuration
    #[validate(nested)]
    pub tracing: TracingConfig,

    /// Structured logging configuration
    #[validate(nested)]
    pub logging: LoggingConfig,

    /// Metrics export configuration
    #[validate(nested)]
    pub export: ExportConfig,

    /// Global performance constraints
    #[validate(nested)]
    pub performance: PerformanceConfig,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            metrics: MetricsConfig::default(),
            tracing: TracingConfig::default(),
            logging: LoggingConfig::default(),
            export: ExportConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

/// Metrics collection configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MetricsConfig {
    /// Whether metrics collection is enabled
    pub enabled: bool,

    /// Metrics collection interval
    #[validate(custom = "validate_duration_range")]
    pub collection_interval: Duration,

    /// Metrics retention period
    #[validate(custom = "validate_duration_range")]
    pub retention_period: Duration,

    /// Maximum metric cardinality to prevent memory exhaustion
    #[validate(range(min = 1, max = 1_000_000))]
    pub max_cardinality: usize,

    /// Sampling rate for high-cardinality metrics (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub sampling_rate: f64,

    /// Maximum overhead budget as percentage of system performance (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub overhead_budget: f64,

    /// Histogram bucket configuration
    #[validate(nested)]
    pub histogram: HistogramConfig,

    /// Buffer sizes for metrics collection
    #[validate(nested)]
    pub buffers: BufferConfig,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: Duration::from_millis(100),
            retention_period: Duration::from_secs(3600), // 1 hour
            max_cardinality: 100_000,
            sampling_rate: 1.0,
            overhead_budget: 0.01, // 1% maximum overhead
            histogram: HistogramConfig::default(),
            buffers: BufferConfig::default(),
        }
    }
}

/// Histogram-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HistogramConfig {
    /// Number of histogram buckets
    #[validate(range(min = 8, max = 256))]
    pub bucket_count: usize,

    /// Histogram precision mode
    pub precision: HistogramPrecision,

    /// Custom bucket boundaries for latency measurements (microseconds)
    pub latency_buckets: Option<Vec<f64>>,

    /// Custom bucket boundaries for throughput measurements
    pub throughput_buckets: Option<Vec<f64>>,
}

impl Default for HistogramConfig {
    fn default() -> Self {
        Self {
            bucket_count: 64,
            precision: HistogramPrecision::High,
            latency_buckets: Some(vec![
                1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0,
                2500.0, 5000.0, 10000.0, 25000.0, 50000.0, 100000.0,
            ]),
            throughput_buckets: Some(vec![
                100.0, 500.0, 1000.0, 5000.0, 10000.0, 50000.0, 100000.0,
                500000.0, 1000000.0, 5000000.0, 10000000.0,
            ]),
        }
    }
}

/// Histogram precision modes
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum HistogramPrecision {
    /// Low precision, high performance
    Low,
    /// Medium precision, balanced performance
    Medium,
    /// High precision, optimized for accuracy
    High,
    /// Ultra precision with SIMD optimizations
    Ultra,
}

/// Buffer configuration for metrics collection
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BufferConfig {
    /// Ring buffer size for metrics (power of 2 for optimal performance)
    #[validate(custom = "validate_power_of_two")]
    pub metrics_buffer_size: usize,

    /// Batch size for metrics processing
    #[validate(range(min = 1, max = 10000))]
    pub batch_size: usize,

    /// Buffer flush interval
    #[validate(custom = "validate_duration_range")]
    pub flush_interval: Duration,

    /// Memory pool size for metric objects
    #[validate(range(min = 1024, max = 100_000_000))]
    pub memory_pool_size: usize,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            metrics_buffer_size: 65536, // 64K entries
            batch_size: 1000,
            flush_interval: Duration::from_millis(100),
            memory_pool_size: 10_000_000, // 10MB
        }
    }
}

/// Distributed tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TracingConfig {
    /// Whether tracing is enabled
    pub enabled: bool,

    /// Trace sampling rate (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub sampling_rate: f64,

    /// Maximum spans per trace to prevent memory exhaustion
    #[validate(range(min = 1, max = 10000))]
    pub max_spans_per_trace: usize,

    /// Trace timeout for automatic cleanup
    #[validate(custom = "validate_duration_range")]
    pub trace_timeout: Duration,

    /// Span batch size for export
    #[validate(range(min = 1, max = 10000))]
    pub batch_size: usize,

    /// Span buffer configuration
    #[validate(nested)]
    pub buffer: SpanBufferConfig,

    /// OpenTelemetry configuration
    #[validate(nested)]
    pub opentelemetry: OpenTelemetryConfig,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling_rate: 0.01, // 1% sampling by default
            max_spans_per_trace: 1000,
            trace_timeout: Duration::from_secs(300), // 5 minutes
            batch_size: 100,
            buffer: SpanBufferConfig::default(),
            opentelemetry: OpenTelemetryConfig::default(),
        }
    }
}

/// Span buffer configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SpanBufferConfig {
    /// Maximum spans in buffer
    #[validate(range(min = 100, max = 1_000_000))]
    pub max_spans: usize,

    /// Buffer flush interval
    #[validate(custom = "validate_duration_range")]
    pub flush_interval: Duration,

    /// Span export timeout
    #[validate(custom = "validate_duration_range")]
    pub export_timeout: Duration,
}

impl Default for SpanBufferConfig {
    fn default() -> Self {
        Self {
            max_spans: 10000,
            flush_interval: Duration::from_secs(5),
            export_timeout: Duration::from_secs(30),
        }
    }
}

/// OpenTelemetry specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OpenTelemetryConfig {
    /// Service name for traces
    pub service_name: String,

    /// Service version
    pub service_version: String,

    /// Resource attributes
    pub resource_attributes: HashMap<String, String>,

    /// OTLP endpoint configuration
    pub otlp: Option<OtlpConfig>,

    /// Jaeger endpoint configuration
    pub jaeger: Option<JaegerConfig>,
}

impl Default for OpenTelemetryConfig {
    fn default() -> Self {
        let mut resource_attributes = HashMap::new();
        resource_attributes.insert("service.name".to_string(), "kaelix".to_string());
        resource_attributes.insert("service.version".to_string(), env!("CARGO_PKG_VERSION").to_string());

        Self {
            service_name: "kaelix".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            resource_attributes,
            otlp: None,
            jaeger: None,
        }
    }
}

/// OTLP (OpenTelemetry Protocol) configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OtlpConfig {
    /// OTLP endpoint URL
    #[validate(url)]
    pub endpoint: String,

    /// Authentication headers
    pub headers: HashMap<String, String>,

    /// Request timeout
    #[validate(custom = "validate_duration_range")]
    pub timeout: Duration,

    /// Compression method
    pub compression: CompressionMethod,
}

/// Jaeger tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct JaegerConfig {
    /// Jaeger agent endpoint
    pub agent_endpoint: String,

    /// Jaeger collector endpoint
    #[validate(url)]
    pub collector_endpoint: Option<String>,

    /// Authentication credentials
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Compression methods for trace export
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompressionMethod {
    None,
    Gzip,
    Deflate,
    Brotli,
}

/// Structured logging configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LoggingConfig {
    /// Whether logging is enabled
    pub enabled: bool,

    /// Log level filtering
    pub level: LogLevel,

    /// Log output format
    pub format: LogFormat,

    /// Async buffer size for log entries
    #[validate(custom = "validate_power_of_two")]
    pub async_buffer_size: usize,

    /// Log flush interval
    #[validate(custom = "validate_duration_range")]
    pub flush_interval: Duration,

    /// Whether to use structured logging
    pub structured_logging: bool,

    /// Log file configuration
    pub file: Option<LogFileConfig>,

    /// Console logging configuration
    #[validate(nested)]
    pub console: ConsoleLogConfig,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: LogLevel::Info,
            format: LogFormat::Json,
            async_buffer_size: 8192,
            flush_interval: Duration::from_millis(100),
            structured_logging: true,
            file: None,
            console: ConsoleLogConfig::default(),
        }
    }
}

/// Log levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace => write!(f, "trace"),
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Log output formats
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogFormat {
    /// Human-readable text format
    Text,
    /// JSON structured format
    Json,
    /// Compact binary format
    Binary,
    /// Custom format
    Custom,
}

/// Log file configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LogFileConfig {
    /// Log file path
    pub path: String,

    /// Maximum file size before rotation
    #[validate(range(min = 1024, max = 10_000_000_000))]
    pub max_size_bytes: u64,

    /// Number of rotated files to keep
    #[validate(range(min = 1, max = 100))]
    pub max_files: usize,

    /// File rotation strategy
    pub rotation: RotationStrategy,
}

/// File rotation strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RotationStrategy {
    Size,
    Time,
    Both,
}

/// Console logging configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConsoleLogConfig {
    /// Whether console logging is enabled
    pub enabled: bool,

    /// Whether to use colored output
    pub colored: bool,

    /// Whether to include timestamps
    pub timestamps: bool,

    /// Whether to include module paths
    pub module_paths: bool,
}

impl Default for ConsoleLogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            colored: true,
            timestamps: true,
            module_paths: false,
        }
    }
}

/// Metrics export configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExportConfig {
    /// Whether metrics export is enabled
    pub enabled: bool,

    /// Export targets
    #[validate(length(max = 10))]
    pub targets: Vec<ExportTarget>,

    /// Global export settings
    #[validate(nested)]
    pub settings: ExportSettings,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            targets: vec![],
            settings: ExportSettings::default(),
        }
    }
}

/// Export target configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type")]
pub enum ExportTarget {
    #[serde(rename = "prometheus")]
    Prometheus {
        #[validate(url)]
        endpoint: String,
        #[validate(custom = "validate_duration_range")]
        interval: Duration,
        basic_auth: Option<BasicAuth>,
    },
    #[serde(rename = "opentelemetry")]
    OpenTelemetry {
        #[validate(url)]
        endpoint: String,
        headers: HashMap<String, String>,
        #[validate(custom = "validate_duration_range")]
        timeout: Duration,
    },
    #[serde(rename = "influxdb")]
    InfluxDB {
        #[validate(url)]
        endpoint: String,
        database: String,
        retention_policy: Option<String>,
        basic_auth: Option<BasicAuth>,
        #[validate(custom = "validate_duration_range")]
        timeout: Duration,
    },
    #[serde(rename = "custom")]
    Custom {
        name: String,
        config: HashMap<String, serde_json::Value>,
    },
}

/// Basic authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

/// Export settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExportSettings {
    /// Default export interval
    #[validate(custom = "validate_duration_range")]
    pub default_interval: Duration,

    /// Export batch size
    #[validate(range(min = 1, max = 100000))]
    pub batch_size: usize,

    /// Export timeout
    #[validate(custom = "validate_duration_range")]
    pub timeout: Duration,

    /// Maximum retries for failed exports
    #[validate(range(min = 0, max = 10))]
    pub max_retries: u32,

    /// Retry backoff configuration
    #[validate(nested)]
    pub retry_backoff: RetryBackoffConfig,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            default_interval: Duration::from_secs(60),
            batch_size: 1000,
            timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_backoff: RetryBackoffConfig::default(),
        }
    }
}

/// Retry backoff configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RetryBackoffConfig {
    /// Initial backoff delay
    #[validate(custom = "validate_duration_range")]
    pub initial_delay: Duration,

    /// Maximum backoff delay
    #[validate(custom = "validate_duration_range")]
    pub max_delay: Duration,

    /// Backoff multiplier
    #[validate(range(min = 1.0, max = 10.0))]
    pub multiplier: f64,

    /// Whether to add jitter to backoff delays
    pub jitter: bool,
}

impl Default for RetryBackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

/// Performance configuration and constraints
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PerformanceConfig {
    /// Maximum total telemetry overhead as percentage (0.0-1.0)
    #[validate(range(min = 0.0, max = 0.1))] // Max 10%
    pub max_overhead_percent: f64,

    /// Target metric recording latency in nanoseconds
    #[validate(range(min = 10, max = 10000))]
    pub target_metric_latency_ns: u64,

    /// Memory limits
    #[validate(nested)]
    pub memory: MemoryLimits,

    /// CPU limits
    #[validate(nested)]
    pub cpu: CpuLimits,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_overhead_percent: 0.01, // 1%
            target_metric_latency_ns: 100, // 100ns
            memory: MemoryLimits::default(),
            cpu: CpuLimits::default(),
        }
    }
}

/// Memory usage limits
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryLimits {
    /// Maximum memory usage in bytes
    #[validate(range(min = 1_000_000, max = 1_000_000_000))] // 1MB - 1GB
    pub max_usage_bytes: usize,

    /// Memory pressure threshold (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub pressure_threshold: f64,

    /// Memory cleanup strategy
    pub cleanup_strategy: MemoryCleanupStrategy,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            max_usage_bytes: 10_000_000, // 10MB
            pressure_threshold: 0.8, // 80%
            cleanup_strategy: MemoryCleanupStrategy::Adaptive,
        }
    }
}

/// Memory cleanup strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MemoryCleanupStrategy {
    /// No automatic cleanup
    None,
    /// Cleanup oldest data first
    Fifo,
    /// Cleanup least recently used data
    Lru,
    /// Adaptive cleanup based on usage patterns
    Adaptive,
}

/// CPU usage limits
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CpuLimits {
    /// Maximum CPU usage percentage (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub max_usage_percent: f64,

    /// CPU usage measurement interval
    #[validate(custom = "validate_duration_range")]
    pub measurement_interval: Duration,

    /// CPU throttling strategy
    pub throttling_strategy: CpuThrottlingStrategy,
}

impl Default for CpuLimits {
    fn default() -> Self {
        Self {
            max_usage_percent: 0.01, // 1%
            measurement_interval: Duration::from_secs(1),
            throttling_strategy: CpuThrottlingStrategy::Adaptive,
        }
    }
}

/// CPU throttling strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CpuThrottlingStrategy {
    /// No throttling
    None,
    /// Reduce sampling rates
    ReduceSampling,
    /// Increase batch sizes
    IncreaseBatching,
    /// Adaptive throttling
    Adaptive,
}

// Validation functions

/// Validates duration is within reasonable range
fn validate_duration_range(duration: &Duration) -> Result<(), ValidationError> {
    let millis = duration.as_millis();
    if millis < 1 || millis > 86_400_000 { // 1ms to 24 hours
        return Err(ValidationError::new("duration_out_of_range"));
    }
    Ok(())
}

/// Validates value is a power of two
fn validate_power_of_two(value: &usize) -> Result<(), ValidationError> {
    if *value == 0 || (*value & (*value - 1)) != 0 {
        return Err(ValidationError::new("not_power_of_two"));
    }
    Ok(())
}

impl TelemetryConfig {
    /// Validates the configuration and returns detailed errors
    pub fn validate_config(&self) -> Result<()> {
        // Perform validator validation first
        self.validate()
            .map_err(|e| TelemetryError::config(format!("Validation failed: {}", e)))?;

        // Custom business logic validation
        self.validate_business_rules()?;

        Ok(())
    }

    /// Validates business rules and constraints
    fn validate_business_rules(&self) -> Result<()> {
        // Validate performance constraints
        if self.performance.max_overhead_percent > 0.05 {
            return Err(TelemetryError::config(
                "Maximum overhead cannot exceed 5% for production systems"
            ));
        }

        // Validate tracing sampling rate vs performance budget
        if self.tracing.sampling_rate > 0.1 && self.performance.max_overhead_percent < 0.02 {
            return Err(TelemetryError::config(
                "High tracing sampling rate incompatible with low overhead budget"
            ));
        }

        // Validate export target consistency
        if self.export.enabled && self.export.targets.is_empty() {
            return Err(TelemetryError::config(
                "Export enabled but no export targets configured"
            ));
        }

        // Validate buffer sizes are reasonable
        if self.metrics.buffers.metrics_buffer_size > 1_000_000 {
            return Err(TelemetryError::config(
                "Metrics buffer size too large, may cause memory issues"
            ));
        }

        Ok(())
    }

    /// Creates a high-performance configuration optimized for minimal overhead
    pub fn high_performance() -> Self {
        Self {
            enabled: true,
            metrics: MetricsConfig {
                enabled: true,
                collection_interval: Duration::from_millis(50),
                retention_period: Duration::from_secs(1800), // 30 minutes
                max_cardinality: 50_000,
                sampling_rate: 1.0,
                overhead_budget: 0.005, // 0.5%
                histogram: HistogramConfig {
                    bucket_count: 32,
                    precision: HistogramPrecision::Medium,
                    ..Default::default()
                },
                buffers: BufferConfig {
                    metrics_buffer_size: 32768,
                    batch_size: 2000,
                    flush_interval: Duration::from_millis(50),
                    memory_pool_size: 5_000_000,
                },
            },
            tracing: TracingConfig {
                enabled: true,
                sampling_rate: 0.001, // 0.1% sampling
                max_spans_per_trace: 500,
                trace_timeout: Duration::from_secs(120),
                batch_size: 500,
                ..Default::default()
            },
            logging: LoggingConfig {
                enabled: true,
                level: LogLevel::Warn, // Reduced logging
                format: LogFormat::Binary, // Faster serialization
                async_buffer_size: 16384,
                flush_interval: Duration::from_millis(50),
                structured_logging: false, // Faster plain text
                file: None,
                console: ConsoleLogConfig {
                    enabled: false, // Disable console for production
                    ..Default::default()
                },
            },
            export: ExportConfig {
                enabled: false, // Disable exports for maximum performance
                targets: vec![],
                settings: ExportSettings {
                    default_interval: Duration::from_secs(300), // 5 minutes
                    batch_size: 5000,
                    timeout: Duration::from_secs(10),
                    max_retries: 1,
                    ..Default::default()
                },
            },
            performance: PerformanceConfig {
                max_overhead_percent: 0.005, // 0.5%
                target_metric_latency_ns: 50, // 50ns target
                memory: MemoryLimits {
                    max_usage_bytes: 5_000_000, // 5MB
                    pressure_threshold: 0.9,
                    cleanup_strategy: MemoryCleanupStrategy::Adaptive,
                },
                cpu: CpuLimits {
                    max_usage_percent: 0.005, // 0.5%
                    measurement_interval: Duration::from_secs(1),
                    throttling_strategy: CpuThrottlingStrategy::Adaptive,
                },
            },
        }
    }

    /// Creates a development configuration with verbose logging and debugging
    pub fn development() -> Self {
        Self {
            enabled: true,
            metrics: MetricsConfig {
                enabled: true,
                collection_interval: Duration::from_millis(200),
                retention_period: Duration::from_secs(7200), // 2 hours
                max_cardinality: 200_000,
                sampling_rate: 1.0,
                overhead_budget: 0.05, // 5% acceptable in dev
                histogram: HistogramConfig {
                    bucket_count: 64,
                    precision: HistogramPrecision::High,
                    ..Default::default()
                },
                ..Default::default()
            },
            tracing: TracingConfig {
                enabled: true,
                sampling_rate: 0.1, // 10% sampling in dev
                max_spans_per_trace: 2000,
                trace_timeout: Duration::from_secs(600),
                batch_size: 50,
                ..Default::default()
            },
            logging: LoggingConfig {
                enabled: true,
                level: LogLevel::Debug, // Verbose logging
                format: LogFormat::Json,
                async_buffer_size: 4096,
                flush_interval: Duration::from_millis(200),
                structured_logging: true,
                file: Some(LogFileConfig {
                    path: "logs/kaelix-dev.log".to_string(),
                    max_size_bytes: 100_000_000, // 100MB
                    max_files: 5,
                    rotation: RotationStrategy::Size,
                }),
                console: ConsoleLogConfig {
                    enabled: true,
                    colored: true,
                    timestamps: true,
                    module_paths: true,
                },
            },
            export: ExportConfig {
                enabled: true,
                targets: vec![],
                settings: ExportSettings {
                    default_interval: Duration::from_secs(30),
                    batch_size: 100,
                    timeout: Duration::from_secs(10),
                    max_retries: 3,
                    ..Default::default()
                },
            },
            performance: PerformanceConfig {
                max_overhead_percent: 0.05, // 5% in development
                target_metric_latency_ns: 200, // Relaxed target
                memory: MemoryLimits {
                    max_usage_bytes: 50_000_000, // 50MB
                    pressure_threshold: 0.7,
                    cleanup_strategy: MemoryCleanupStrategy::Fifo,
                },
                cpu: CpuLimits {
                    max_usage_percent: 0.05, // 5%
                    measurement_interval: Duration::from_secs(1),
                    throttling_strategy: CpuThrottlingStrategy::None,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_validation() {
        let config = TelemetryConfig::default();
        config.validate_config().unwrap();
    }

    #[test]
    fn test_high_performance_config() {
        let config = TelemetryConfig::high_performance();
        config.validate_config().unwrap();
        
        assert_eq!(config.performance.max_overhead_percent, 0.005);
        assert_eq!(config.tracing.sampling_rate, 0.001);
        assert_eq!(config.logging.level, LogLevel::Warn);
    }

    #[test]
    fn test_development_config() {
        let config = TelemetryConfig::development();
        config.validate_config().unwrap();
        
        assert_eq!(config.logging.level, LogLevel::Debug);
        assert_eq!(config.tracing.sampling_rate, 0.1);
        assert!(config.logging.console.enabled);
    }

    #[test]
    fn test_validation_errors() {
        let mut config = TelemetryConfig::default();
        config.performance.max_overhead_percent = 0.1; // 10% - too high
        
        assert!(config.validate_config().is_err());
    }

    #[test]
    fn test_power_of_two_validation() {
        assert!(validate_power_of_two(&1024).is_ok());
        assert!(validate_power_of_two(&1023).is_err());
        assert!(validate_power_of_two(&0).is_err());
    }

    #[test]
    fn test_duration_validation() {
        assert!(validate_duration_range(&Duration::from_millis(100)).is_ok());
        assert!(validate_duration_range(&Duration::from_nanos(100)).is_err());
        assert!(validate_duration_range(&Duration::from_secs(100000)).is_err());
    }
}