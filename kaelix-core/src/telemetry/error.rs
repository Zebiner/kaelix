//! # Telemetry Error Handling
//!
//! Comprehensive error handling for the telemetry & observability framework.
//! Designed for minimal overhead with zero-allocation error paths where possible.

use std::fmt;
use thiserror::Error;

/// Main telemetry error type with comprehensive error categories
#[derive(Error, Debug, Clone)]
pub enum TelemetryError {
    /// Configuration errors
    #[error("Configuration error: {message}")]
    Config {
        message: String,
        source: Option<String>,
    },

    /// Metrics collection and recording errors  
    #[error("Metrics error: {message}")]
    Metrics {
        message: String,
        metric_key: Option<String>,
    },

    /// Distributed tracing errors
    #[error("Tracing error: {message}")]
    Tracing {
        message: String,
        trace_id: Option<String>,
        span_id: Option<String>,
    },

    /// Structured logging errors
    #[error("Logging error: {message}")]
    Logging {
        message: String,
        log_level: Option<String>,
    },

    /// Metrics registry errors
    #[error("Registry error: {message}")]
    Registry {
        message: String,
        registry_size: Option<usize>,
    },

    /// Metrics export errors
    #[error("Export error: {message}")]
    Export {
        message: String,
        target: Option<String>,
        retry_count: Option<u32>,
    },

    /// I/O and system errors
    #[error("I/O error: {message}")]
    Io {
        message: String,
        #[source]
        source: std::io::Error,
    },

    /// Serialization/deserialization errors
    #[error("Serialization error: {message}")]
    Serialization {
        message: String,
        format: Option<String>,
    },

    /// Network and transport errors
    #[error("Network error: {message}")]
    Network {
        message: String,
        endpoint: Option<String>,
        status_code: Option<u16>,
    },

    /// Resource exhaustion errors
    #[error("Resource exhaustion: {message}")]
    ResourceExhaustion {
        message: String,
        resource_type: ResourceType,
        current_usage: Option<u64>,
        limit: Option<u64>,
    },

    /// Performance threshold violations
    #[error("Performance violation: {message}")]
    Performance {
        message: String,
        metric_name: String,
        actual_value: f64,
        threshold_value: f64,
    },

    /// Security and validation errors
    #[error("Security error: {message}")]
    Security {
        message: String,
        security_context: Option<String>,
    },

    /// Thread and concurrency errors
    #[error("Concurrency error: {message}")]
    Concurrency {
        message: String,
        thread_name: Option<String>,
    },
}

/// Types of system resources that can be exhausted
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    /// Memory allocation limits
    Memory,
    /// CPU usage limits  
    Cpu,
    /// Network bandwidth limits
    NetworkBandwidth,
    /// Disk space limits
    DiskSpace,
    /// File descriptor limits
    FileDescriptors,
    /// Thread pool limits
    ThreadPool,
    /// Queue capacity limits
    QueueCapacity,
    /// Connection pool limits
    ConnectionPool,
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory => write!(f, "memory"),
            Self::Cpu => write!(f, "cpu"),
            Self::NetworkBandwidth => write!(f, "network_bandwidth"),
            Self::DiskSpace => write!(f, "disk_space"),
            Self::FileDescriptors => write!(f, "file_descriptors"),
            Self::ThreadPool => write!(f, "thread_pool"),
            Self::QueueCapacity => write!(f, "queue_capacity"),
            Self::ConnectionPool => write!(f, "connection_pool"),
        }
    }
}

impl TelemetryError {
    /// Creates a new configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Config {
            message: message.into(),
            source: None,
        }
    }

    /// Creates a configuration error with source information
    pub fn config_with_source<S: Into<String>>(message: S, source: S) -> Self {
        Self::Config {
            message: message.into(),
            source: Some(source.into()),
        }
    }

    /// Creates a new metrics error
    pub fn metrics<S: Into<String>>(message: S) -> Self {
        Self::Metrics {
            message: message.into(),
            metric_key: None,
        }
    }

    /// Creates a metrics error with metric key context
    pub fn metrics_with_key<S: Into<String>>(message: S, metric_key: S) -> Self {
        Self::Metrics {
            message: message.into(),
            metric_key: Some(metric_key.into()),
        }
    }

    /// Creates a new tracing error
    pub fn tracing<S: Into<String>>(message: S) -> Self {
        Self::Tracing {
            message: message.into(),
            trace_id: None,
            span_id: None,
        }
    }

    /// Creates a tracing error with trace context
    pub fn tracing_with_context<S: Into<String>>(
        message: S,
        trace_id: Option<S>,
        span_id: Option<S>,
    ) -> Self {
        Self::Tracing {
            message: message.into(),
            trace_id: trace_id.map(Into::into),
            span_id: span_id.map(Into::into),
        }
    }

    /// Creates a new logging error
    pub fn logging<S: Into<String>>(message: S) -> Self {
        Self::Logging {
            message: message.into(),
            log_level: None,
        }
    }

    /// Creates a logging error with log level context
    pub fn logging_with_level<S: Into<String>>(message: S, log_level: S) -> Self {
        Self::Logging {
            message: message.into(),
            log_level: Some(log_level.into()),
        }
    }

    /// Creates a new registry error
    pub fn registry<S: Into<String>>(message: S) -> Self {
        Self::Registry {
            message: message.into(),
            registry_size: None,
        }
    }

    /// Creates a registry error with size context
    pub fn registry_with_size<S: Into<String>>(message: S, size: usize) -> Self {
        Self::Registry {
            message: message.into(),
            registry_size: Some(size),
        }
    }

    /// Creates a new export error
    pub fn export<S: Into<String>>(message: S) -> Self {
        Self::Export {
            message: message.into(),
            target: None,
            retry_count: None,
        }
    }

    /// Creates an export error with target and retry context
    pub fn export_with_context<S: Into<String>>(
        message: S,
        target: Option<S>,
        retry_count: Option<u32>,
    ) -> Self {
        Self::Export {
            message: message.into(),
            target: target.map(Into::into),
            retry_count,
        }
    }

    /// Creates a new I/O error
    pub fn io<S: Into<String>>(message: S, source: std::io::Error) -> Self {
        Self::Io {
            message: message.into(),
            source,
        }
    }

    /// Creates a new serialization error
    pub fn serialization<S: Into<String>>(message: S) -> Self {
        Self::Serialization {
            message: message.into(),
            format: None,
        }
    }

    /// Creates a serialization error with format context
    pub fn serialization_with_format<S: Into<String>>(message: S, format: S) -> Self {
        Self::Serialization {
            message: message.into(),
            format: Some(format.into()),
        }
    }

    /// Creates a new network error
    pub fn network<S: Into<String>>(message: S) -> Self {
        Self::Network {
            message: message.into(),
            endpoint: None,
            status_code: None,
        }
    }

    /// Creates a network error with endpoint and status context
    pub fn network_with_context<S: Into<String>>(
        message: S,
        endpoint: Option<S>,
        status_code: Option<u16>,
    ) -> Self {
        Self::Network {
            message: message.into(),
            endpoint: endpoint.map(Into::into),
            status_code,
        }
    }

    /// Creates a new resource exhaustion error
    pub fn resource_exhaustion<S: Into<String>>(
        message: S,
        resource_type: ResourceType,
    ) -> Self {
        Self::ResourceExhaustion {
            message: message.into(),
            resource_type,
            current_usage: None,
            limit: None,
        }
    }

    /// Creates a resource exhaustion error with usage context
    pub fn resource_exhaustion_with_usage<S: Into<String>>(
        message: S,
        resource_type: ResourceType,
        current_usage: u64,
        limit: u64,
    ) -> Self {
        Self::ResourceExhaustion {
            message: message.into(),
            resource_type,
            current_usage: Some(current_usage),
            limit: Some(limit),
        }
    }

    /// Creates a new performance violation error
    pub fn performance_violation<S: Into<String>>(
        message: S,
        metric_name: S,
        actual_value: f64,
        threshold_value: f64,
    ) -> Self {
        Self::Performance {
            message: message.into(),
            metric_name: metric_name.into(),
            actual_value,
            threshold_value,
        }
    }

    /// Creates a new security error
    pub fn security<S: Into<String>>(message: S) -> Self {
        Self::Security {
            message: message.into(),
            security_context: None,
        }
    }

    /// Creates a security error with security context
    pub fn security_with_context<S: Into<String>>(message: S, context: S) -> Self {
        Self::Security {
            message: message.into(),
            security_context: Some(context.into()),
        }
    }

    /// Creates a new concurrency error
    pub fn concurrency<S: Into<String>>(message: S) -> Self {
        Self::Concurrency {
            message: message.into(),
            thread_name: None,
        }
    }

    /// Creates a concurrency error with thread context
    pub fn concurrency_with_thread<S: Into<String>>(message: S, thread_name: S) -> Self {
        Self::Concurrency {
            message: message.into(),
            thread_name: Some(thread_name.into()),
        }
    }

    /// Returns true if this is a retryable error
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network { .. } => true,
            Self::Export { .. } => true,
            Self::Io { .. } => true,
            Self::ResourceExhaustion { resource_type, .. } => {
                matches!(resource_type, ResourceType::NetworkBandwidth | ResourceType::QueueCapacity)
            }
            _ => false,
        }
    }

    /// Returns true if this is a critical error that should stop the system
    pub fn is_critical(&self) -> bool {
        match self {
            Self::Security { .. } => true,
            Self::ResourceExhaustion { resource_type, .. } => {
                matches!(resource_type, ResourceType::Memory | ResourceType::FileDescriptors)
            }
            Self::Performance { actual_value, threshold_value, .. } => {
                // Critical if performance is extremely degraded (>10x threshold)
                *actual_value > *threshold_value * 10.0
            }
            _ => false,
        }
    }

    /// Returns the error category for metrics collection
    pub fn category(&self) -> &'static str {
        match self {
            Self::Config { .. } => "config",
            Self::Metrics { .. } => "metrics",
            Self::Tracing { .. } => "tracing",
            Self::Logging { .. } => "logging",
            Self::Registry { .. } => "registry",
            Self::Export { .. } => "export",
            Self::Io { .. } => "io",
            Self::Serialization { .. } => "serialization",
            Self::Network { .. } => "network",
            Self::ResourceExhaustion { .. } => "resource_exhaustion",
            Self::Performance { .. } => "performance",
            Self::Security { .. } => "security",
            Self::Concurrency { .. } => "concurrency",
        }
    }
}

/// Result type alias for telemetry operations
pub type Result<T> = std::result::Result<T, TelemetryError>;

/// Error context for enhanced error reporting with minimal allocation
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Component that generated the error
    pub component: &'static str,
    /// Operation that failed
    pub operation: &'static str,
    /// Additional context key-value pairs
    pub context: smallvec::SmallVec<[(&'static str, String); 4]>,
}

impl ErrorContext {
    /// Creates a new error context
    pub fn new(component: &'static str, operation: &'static str) -> Self {
        Self {
            component,
            operation,
            context: smallvec::SmallVec::new(),
        }
    }

    /// Adds context information
    pub fn with_context<S: Into<String>>(mut self, key: &'static str, value: S) -> Self {
        self.context.push((key, value.into()));
        self
    }
}

/// Conversion from standard I/O errors
impl From<std::io::Error> for TelemetryError {
    fn from(err: std::io::Error) -> Self {
        Self::io("I/O error occurred", err)
    }
}

/// Conversion from serde JSON errors
impl From<serde_json::Error> for TelemetryError {
    fn from(err: serde_json::Error) -> Self {
        Self::serialization_with_format(err.to_string(), "json")
    }
}

/// Conversion from tokio join errors
impl From<tokio::task::JoinError> for TelemetryError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::concurrency(format!("Task join error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = TelemetryError::config("Invalid configuration");
        assert_eq!(err.category(), "config");
        assert!(!err.is_retryable());
        assert!(!err.is_critical());
    }

    #[test]
    fn test_error_with_context() {
        let err = TelemetryError::metrics_with_key("Failed to record metric", "test.counter");
        match err {
            TelemetryError::Metrics { metric_key: Some(key), .. } => {
                assert_eq!(key, "test.counter");
            }
            _ => panic!("Expected metrics error with key"),
        }
    }

    #[test]
    fn test_retryable_errors() {
        let network_err = TelemetryError::network("Connection failed");
        assert!(network_err.is_retryable());

        let config_err = TelemetryError::config("Invalid setting");
        assert!(!config_err.is_retryable());
    }

    #[test]
    fn test_critical_errors() {
        let security_err = TelemetryError::security("Authentication failed");
        assert!(security_err.is_critical());

        let memory_err = TelemetryError::resource_exhaustion_with_usage(
            "Out of memory",
            ResourceType::Memory,
            1_000_000_000,
            1_000_000_000,
        );
        assert!(memory_err.is_critical());
    }

    #[test]
    fn test_performance_error_criticality() {
        let critical_perf = TelemetryError::performance_violation(
            "Severe performance degradation",
            "latency",
            1000.0,
            10.0, // 100x threshold = critical
        );
        assert!(critical_perf.is_critical());

        let minor_perf = TelemetryError::performance_violation(
            "Minor performance degradation",
            "latency",
            50.0,
            10.0, // 5x threshold = not critical
        );
        assert!(!minor_perf.is_critical());
    }

    #[test]
    fn test_error_context() {
        let ctx = ErrorContext::new("metrics", "record")
            .with_context("metric_key", "test.counter")
            .with_context("value", "42");

        assert_eq!(ctx.component, "metrics");
        assert_eq!(ctx.operation, "record");
        assert_eq!(ctx.context.len(), 2);
        assert_eq!(ctx.context[0].0, "metric_key");
        assert_eq!(ctx.context[0].1, "test.counter");
    }

    #[test]
    fn test_resource_type_display() {
        assert_eq!(ResourceType::Memory.to_string(), "memory");
        assert_eq!(ResourceType::NetworkBandwidth.to_string(), "network_bandwidth");
        assert_eq!(ResourceType::QueueCapacity.to_string(), "queue_capacity");
    }

    #[test]
    fn test_error_conversions() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let tel_err: TelemetryError = io_err.into();
        match tel_err {
            TelemetryError::Io { .. } => (),
            _ => panic!("Expected I/O error conversion"),
        }
    }
}