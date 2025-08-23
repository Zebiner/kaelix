//! Configuration types and validation for telemetry components

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use url::Url;
use validator::{Validate, ValidationError, ValidationErrors};

/// Global telemetry configuration combining all subsystems
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TelemetryConfig {
    /// Metrics collection configuration
    #[validate(nested)]
    pub metrics: MetricsConfig,

    /// Logging configuration
    #[validate(nested)]
    pub logging: LoggingConfig,

    /// Tracing configuration
    #[validate(nested)]
    pub tracing: TracingConfig,

    /// Export configuration
    #[validate(nested)]
    pub export: ExportConfig,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            metrics: MetricsConfig::default(),
            logging: LoggingConfig::default(),
            tracing: TracingConfig::default(),
            export: ExportConfig::default(),
        }
    }
}

/// Configuration for metrics collection
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MetricsConfig {
    /// Whether metrics collection is enabled
    pub enabled: bool,

    /// Collection interval in milliseconds
    #[validate(range(min = 100, max = 60_000))]
    pub collection_interval_ms: u64,

    /// Maximum number of metric keys to store
    #[validate(range(min = 1000, max = 1_000_000))]
    pub max_metric_keys: usize,

    /// Sample retention duration in seconds
    #[validate(range(min = 60, max = 86_400))]
    pub retention_seconds: u64,

    /// Metrics export targets
    pub targets: Vec<String>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval_ms: 1000,
            max_metric_keys: 100_000,
            retention_seconds: 3600,
            targets: vec!["prometheus".to_string()],
        }
    }
}

/// Configuration for structured logging
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LoggingConfig {
    /// Whether logging is enabled
    pub enabled: bool,

    /// Minimum log level
    pub level: LogLevel,

    /// Log output format
    pub format: LogFormat,

    /// Log file path (optional - use stdout if None)
    pub file_path: Option<String>,

    /// Log file rotation size in MB (0 = no rotation)
    #[validate(range(min = 0, max = 1024))]
    pub rotation_size_mb: u64,

    /// Maximum number of log files to keep
    #[validate(range(min = 1, max = 100))]
    pub max_files: u32,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: LogLevel::Info,
            format: LogFormat::Json,
            file_path: None,
            rotation_size_mb: 100,
            max_files: 10,
        }
    }
}

/// Log levels in order of increasing severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Log output formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogFormat {
    /// Human-readable text format
    Text,
    /// Structured JSON format
    Json,
    /// Compact binary format
    Binary,
}

/// Configuration for distributed tracing
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TracingConfig {
    /// Whether tracing is enabled
    pub enabled: bool,

    /// Sampling strategy
    pub sampling_strategy: SamplingStrategy,

    /// Maximum span duration in milliseconds before timeout
    #[validate(range(min = 1000, max = 300_000))]
    pub max_span_duration_ms: u64,

    /// Maximum number of active traces
    #[validate(range(min = 100, max = 10_000))]
    pub max_active_traces: usize,

    /// Batch size for span export
    #[validate(range(min = 10, max = 1000))]
    pub batch_size: usize,

    /// Export endpoint for traces
    pub export_endpoint: Option<String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling_strategy: SamplingStrategy::Percentage(0.1),
            max_span_duration_ms: 30_000,
            max_active_traces: 1000,
            batch_size: 100,
            export_endpoint: None,
        }
    }
}

/// Sampling strategies for distributed tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SamplingStrategy {
    /// Always sample all traces
    AlwaysOn,
    /// Never sample traces
    AlwaysOff,
    /// Sample a fixed percentage of traces (0.0 - 1.0)
    Percentage(f64),
    /// Rate-limited sampling (traces per second)
    RateLimit(u64),
    /// Adaptive sampling based on system load
    Adaptive,
}

/// Configuration for telemetry data export
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExportConfig {
    /// Whether export is enabled
    pub enabled: bool,

    /// Export format
    pub format: ExportFormat,

    /// Compression algorithm for export data
    pub compression: CompressionType,

    /// Export interval in seconds
    #[validate(range(min = 1, max = 3600))]
    pub interval_seconds: u64,

    /// Maximum batch size for exports
    #[validate(range(min = 1, max = 10_000))]
    pub batch_size: usize,

    /// Export timeout in seconds
    #[validate(range(min = 1, max = 300))]
    pub timeout_seconds: u64,

    /// Retry configuration
    #[validate(nested)]
    pub retry: RetryConfig,

    /// Export targets
    #[validate(nested)]
    pub targets: Vec<ExportTarget>,

    /// Export interval in duration form
    pub export_interval: Duration,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: ExportFormat::Prometheus,
            compression: CompressionType::Gzip,
            interval_seconds: 60,
            batch_size: 1000,
            timeout_seconds: 30,
            retry: RetryConfig::default(),
            targets: Vec::new(),
            export_interval: Duration::from_secs(60),
        }
    }
}

/// Export target configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExportTarget {
    /// Target name/identifier
    pub name: String,

    /// Target type
    pub target_type: ExportTargetType,

    /// Target endpoint URL
    #[validate(url)]
    pub endpoint: String,

    /// Authentication configuration
    pub auth: Option<BasicAuth>,

    /// Custom headers
    pub headers: HashMap<String, String>,

    /// Enable this target
    pub enabled: bool,

    /// Maximum buffer size for this target
    #[validate(range(min = 1000, max = 10_000_000))]
    pub max_buffer_size: usize,

    /// Batch size for this target
    pub batch_size: usize,

    /// Maximum batch delay
    pub max_batch_delay: Duration,
}

impl ExportTarget {
    /// Validate the export target
    pub fn validate(&self) -> std::result::Result<(), ValidationError> {
        validate_url(&self.endpoint)?;
        Ok(())
    }

    /// Get the maximum buffer size for this target
    pub fn max_buffer_size(&self) -> usize {
        self.max_buffer_size
    }

    /// Get the target name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Export target types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportTargetType {
    /// Prometheus metrics endpoint
    Prometheus,
    /// OpenTelemetry Protocol (OTLP)
    Otlp,
    /// StatsD metrics
    StatsD,
    /// InfluxDB time series database
    InfluxDb,
    /// Custom HTTP endpoint
    Http,
}

/// Basic HTTP authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

/// Retry configuration for failed exports
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    #[validate(range(min = 0, max = 10))]
    pub max_attempts: u32,

    /// Initial retry delay in milliseconds
    #[validate(range(min = 100, max = 60_000))]
    pub initial_delay_ms: u64,

    /// Retry delay multiplier
    #[validate(range(min = 1.0, max = 5.0))]
    pub backoff_multiplier: f64,

    /// Maximum retry delay in milliseconds
    #[validate(range(min = 1000, max = 300_000))]
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30_000,
        }
    }
}

/// Export data formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    /// Prometheus text format
    Prometheus,
    /// OpenTelemetry Protocol (OTLP)
    Otlp,
    /// JSON format
    Json,
    /// MessagePack binary format
    MessagePack,
}

/// Compression types for export data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionType {
    /// No compression
    None,
    /// Gzip compression
    Gzip,
    /// LZ4 compression
    Lz4,
    /// Zstandard compression
    Zstd,
}

/// URL validation helper
fn validate_url(url: &str) -> std::result::Result<(), ValidationError> {
    match Url::parse(url) {
        Ok(_) => Ok(()),
        Err(_e) => Err(ValidationError::new("invalid_url")),
    }
}

/// Configuration validation helpers
impl TelemetryConfig {
    /// Validate the entire configuration
    pub fn validate_all(&self) -> std::result::Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if let Err(validation_errors) = self.validate() {
            // Convert ValidationErrors to individual ValidationError items
            for (field, _field_errors) in validation_errors.field_errors() {
                // Use a static string to avoid temporary value issue
                errors.push(ValidationError::new("validation_error"));
            }
        }

        // Additional cross-validation rules
        if !self.export.enabled && !self.export.targets.is_empty() {
            errors.push(ValidationError::new("export_disabled_with_targets"));
        }

        if self.metrics.collection_interval_ms < 100 {
            errors.push(ValidationError::new("collection_interval_too_low"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
