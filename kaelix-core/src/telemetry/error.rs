//! Error types and handling for telemetry operations
//!
//! Provides comprehensive error types for telemetry subsystems including
//! metrics, logging, tracing, and configuration errors with proper error chaining.

use std::fmt;
use thiserror::Error;

/// Primary error type for all telemetry operations
#[derive(Error, Debug)]
pub enum TelemetryError {
    // Configuration errors
    #[error("Configuration error: {message}")]
    Configuration {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Invalid configuration parameter '{parameter}': {message}")]
    InvalidParameter { parameter: String, message: String },

    #[error("Missing required configuration: {message}")]
    MissingConfiguration { message: String },

    // Metrics subsystem errors
    #[error("Metrics error: {message}")]
    Metrics {
        message: String,
        metric_key: Option<String>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Metric collection failed: {message}")]
    Collection { message: String },

    #[error("Metric aggregation failed: {message}")]
    Aggregation { message: String },

    // I/O and network errors
    #[error("I/O error: {message}")]
    Io {
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Export error: {message}")]
    Export { message: String },

    #[error("Network error: {message}")]
    Network { message: String },

    // Serialization and data format errors
    #[error("Serialization error ({format}): {message}")]
    Serialization { message: String, format: String },

    #[error("Data format error: {message}")]
    DataFormat { message: String },

    #[error("Protocol error: {message}")]
    Protocol { message: String },

    // Tracing subsystem errors
    #[error("Tracing error: {message}")]
    Tracing {
        message: String,
        trace_id: Option<String>,
        span_id: Option<String>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Span processing error: {message}")]
    Span { message: String },

    #[error("Sampling error: {message}")]
    Sampling { message: String },

    // Registry and storage errors
    #[error("Registry error: {message}")]
    Registry {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Storage error: {message}")]
    Storage { message: String },

    #[error("Capacity exceeded: {message}")]
    Capacity { message: String },

    // Concurrency and synchronization errors
    #[error("Concurrency error: {message}")]
    Concurrency { message: String },

    #[error("Lock error: {message}")]
    Lock { message: String },

    #[error("Channel error: {message}")]
    Channel { message: String },

    // Resource and system errors
    #[error("Resource unavailable: {message}")]
    Resource { message: String },

    #[error("System error: {message}")]
    System { message: String },

    #[error("Timeout error: {message}")]
    Timeout { message: String },

    // Generic internal error
    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Result type alias for telemetry operations
pub type Result<T> = std::result::Result<T, TelemetryError>;

impl TelemetryError {
    // Configuration error constructors
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Configuration { message: message.into(), source: None }
    }

    pub fn config_with_source<S: Into<String>>(
        message: S,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::Configuration { message: message.into(), source: Some(source) }
    }

    // Metrics error constructors
    pub fn metrics<S: Into<String>>(message: S) -> Self {
        Self::Metrics { message: message.into(), metric_key: None, source: None }
    }

    pub fn metrics_with_key<S: Into<String>>(message: S, metric_key: S) -> Self {
        Self::Metrics { message: message.into(), metric_key: Some(metric_key.into()), source: None }
    }

    pub fn metrics_with_source<S: Into<String>>(
        message: S,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::Metrics { message: message.into(), metric_key: None, source: Some(source) }
    }

    pub fn collection<S: Into<String>>(message: S) -> Self {
        Self::Collection { message: message.into() }
    }

    pub fn aggregation<S: Into<String>>(message: S) -> Self {
        Self::Aggregation { message: message.into() }
    }

    // I/O error constructors
    pub fn io<S: Into<String>>(message: S, source: std::io::Error) -> Self {
        Self::Io { message: message.into(), source: Box::new(source) }
    }

    pub fn export<S: Into<String>>(message: S) -> Self {
        Self::Export { message: message.into() }
    }

    pub fn network<S: Into<String>>(message: S) -> Self {
        Self::Network { message: message.into() }
    }

    // Serialization error constructors
    pub fn serialization<S: Into<String>>(message: S) -> Self {
        Self::Serialization { message: message.into(), format: "unknown".to_string() }
    }

    pub fn serialization_with_format<S: Into<String>>(message: S, format: S) -> Self {
        Self::Serialization { message: message.into(), format: format.into() }
    }

    pub fn data_format<S: Into<String>>(message: S) -> Self {
        Self::DataFormat { message: message.into() }
    }

    pub fn protocol<S: Into<String>>(message: S) -> Self {
        Self::Protocol { message: message.into() }
    }

    // Tracing error constructors
    pub fn tracing<S: Into<String>>(message: S) -> Self {
        Self::Tracing { message: message.into(), trace_id: None, span_id: None, source: None }
    }

    pub fn tracing_with_context<S: Into<String>>(
        message: S,
        trace_id: Option<S>,
        span_id: Option<S>,
    ) -> Self {
        Self::Tracing {
            message: message.into(),
            trace_id: trace_id.map(|s| s.into()),
            span_id: span_id.map(|s| s.into()),
            source: None,
        }
    }

    pub fn span<S: Into<String>>(message: S) -> Self {
        Self::Span { message: message.into() }
    }

    pub fn sampling<S: Into<String>>(message: S) -> Self {
        Self::Sampling { message: message.into() }
    }

    // Registry error constructors
    pub fn registry<S: Into<String>>(message: S) -> Self {
        Self::Registry { message: message.into(), source: None }
    }

    pub fn registry_with_source<S: Into<String>>(
        message: S,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::Registry { message: message.into(), source: Some(source) }
    }

    pub fn storage<S: Into<String>>(message: S) -> Self {
        Self::Storage { message: message.into() }
    }

    pub fn capacity<S: Into<String>>(message: S) -> Self {
        Self::Capacity { message: message.into() }
    }

    // Concurrency error constructors
    pub fn concurrency<S: Into<String>>(message: S) -> Self {
        Self::Concurrency { message: message.into() }
    }

    pub fn lock<S: Into<String>>(message: S) -> Self {
        Self::Lock { message: message.into() }
    }

    pub fn channel<S: Into<String>>(message: S) -> Self {
        Self::Channel { message: message.into() }
    }

    // Resource error constructors
    pub fn resource<S: Into<String>>(message: S) -> Self {
        Self::Resource { message: message.into() }
    }

    pub fn system<S: Into<String>>(message: S) -> Self {
        Self::System { message: message.into() }
    }

    pub fn timeout<S: Into<String>>(message: S) -> Self {
        Self::Timeout { message: message.into() }
    }

    // Generic error constructor
    pub fn internal<S: Into<String>>(message: S) -> Self {
        Self::Internal { message: message.into() }
    }

    // Utility methods
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            Self::Network { .. }
                | Self::Timeout { .. }
                | Self::Resource { .. }
                | Self::Concurrency { .. }
                | Self::Lock { .. }
                | Self::Channel { .. }
        )
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Configuration { .. } => "CONFIG_ERROR",
            Self::InvalidParameter { .. } => "INVALID_PARAM",
            Self::MissingConfiguration { .. } => "MISSING_CONFIG",
            Self::Metrics { .. } => "METRICS_ERROR",
            Self::Collection { .. } => "COLLECTION_ERROR",
            Self::Aggregation { .. } => "AGGREGATION_ERROR",
            Self::Io { .. } => "IO_ERROR",
            Self::Export { .. } => "EXPORT_ERROR",
            Self::Network { .. } => "NETWORK_ERROR",
            Self::Serialization { .. } => "SERIALIZATION_ERROR",
            Self::DataFormat { .. } => "DATA_FORMAT_ERROR",
            Self::Protocol { .. } => "PROTOCOL_ERROR",
            Self::Tracing { .. } => "TRACING_ERROR",
            Self::Span { .. } => "SPAN_ERROR",
            Self::Sampling { .. } => "SAMPLING_ERROR",
            Self::Registry { .. } => "REGISTRY_ERROR",
            Self::Storage { .. } => "STORAGE_ERROR",
            Self::Capacity { .. } => "CAPACITY_ERROR",
            Self::Concurrency { .. } => "CONCURRENCY_ERROR",
            Self::Lock { .. } => "LOCK_ERROR",
            Self::Channel { .. } => "CHANNEL_ERROR",
            Self::Resource { .. } => "RESOURCE_ERROR",
            Self::System { .. } => "SYSTEM_ERROR",
            Self::Timeout { .. } => "TIMEOUT_ERROR",
            Self::Internal { .. } => "INTERNAL_ERROR",
        }
    }

    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Configuration { .. }
            | Self::InvalidParameter { .. }
            | Self::MissingConfiguration { .. } => ErrorSeverity::High,
            Self::System { .. } | Self::Internal { .. } => ErrorSeverity::Critical,
            Self::Network { .. } | Self::Timeout { .. } | Self::Resource { .. } => {
                ErrorSeverity::Medium
            },
            _ => ErrorSeverity::Low,
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::High => write!(f, "HIGH"),
            Self::Critical => write!(f, "CRITICAL"),
        }
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
        Self::serialization_with_format(err.to_string(), "json".to_string())
    }
}

/// Conversion from tokio join errors
impl From<tokio::task::JoinError> for TelemetryError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::concurrency(format!("Task join error: {}", err))
    }
}

/// Type alias for compatibility with existing code
pub type MemoryStreamerError = TelemetryError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = TelemetryError::config("Test configuration error");
        assert_eq!(err.error_code(), "CONFIG_ERROR");
        assert_eq!(err.severity(), ErrorSeverity::High);
    }

    #[test]
    fn test_error_with_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let err = TelemetryError::from(io_err);
        assert_eq!(err.error_code(), "IO_ERROR");
        assert!(err.source().is_some());
    }

    #[test]
    fn test_retriable_errors() {
        assert!(TelemetryError::network("Network error").is_retriable());
        assert!(TelemetryError::timeout("Timeout error").is_retriable());
        assert!(!TelemetryError::config("Config error").is_retriable());
    }

    #[test]
    fn test_error_severity() {
        assert_eq!(TelemetryError::config("").severity(), ErrorSeverity::High);
        assert_eq!(TelemetryError::network("").severity(), ErrorSeverity::Medium);
        assert_eq!(TelemetryError::metrics("").severity(), ErrorSeverity::Low);
        assert_eq!(TelemetryError::internal("").severity(), ErrorSeverity::Critical);
    }

    #[test]
    fn test_metrics_error_with_key() {
        let err = TelemetryError::metrics_with_key("Test error", "test.metric");
        match err {
            TelemetryError::Metrics { metric_key: Some(key), .. } => {
                assert_eq!(key, "test.metric");
            },
            _ => panic!("Expected Metrics error with key"),
        }
    }

    #[test]
    fn test_tracing_error_with_context() {
        let err =
            TelemetryError::tracing_with_context("Test error", Some("trace-123"), Some("span-456"));
        match err {
            TelemetryError::Tracing { trace_id: Some(tid), span_id: Some(sid), .. } => {
                assert_eq!(tid, "trace-123");
                assert_eq!(sid, "span-456");
            },
            _ => panic!("Expected Tracing error with context"),
        }
    }
}
