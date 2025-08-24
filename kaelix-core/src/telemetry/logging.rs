//! Logging system for MemoryStreamer
//!
//! Provides structured logging capabilities with built-in telemetry collection.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};

use crate::runtime::diagnostics::{HealthIndicator, HealthStatus};
use crate::telemetry::{LogFormat, LogLevel, LoggingConfig, Result};

/// Primary interface for the logging system
pub struct LoggingSystem {
    /// Configuration for this logging instance
    config: LoggingConfig,
    /// Channel to send log entries
    sender: mpsc::UnboundedSender<LogEntry>,
    /// System metrics
    metrics: Arc<LoggingMetrics>,
    /// Health indicator for monitoring
    health: Arc<RwLock<HealthStatus>>,
}

/// A single log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Log message level
    pub level: LogLevel,
    /// Target/source of the log
    pub target: String,
    /// Log message content
    pub message: String,
    /// Key-value pairs for structured data
    pub fields: HashMap<String, String>,
    /// Timestamp of the log entry
    pub timestamp: Instant,
    /// Thread ID where the log was generated
    pub thread_id: String, // Changed from ThreadId to String for serialization
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(level: LogLevel, target: String, message: String) -> Self {
        Self {
            level,
            target,
            message,
            fields: HashMap::new(),
            timestamp: Instant::now(),
            thread_id: format!("{:?}", std::thread::current().id()),
        }
    }

    /// Add a field to the log entry
    pub fn with_field(mut self, key: String, value: String) -> Self {
        self.fields.insert(key, value);
        self
    }

    /// Add multiple fields to the log entry
    pub fn with_fields(mut self, fields: HashMap<String, String>) -> Self {
        self.fields.extend(fields);
        self
    }

    /// Format the log entry as a string
    pub fn format(&self, format: &LogFormat) -> String {
        match format {
            LogFormat::Json => {
                // Simple JSON formatting for now
                format!(
                    r#"{{"level":"{}","target":"{}","message":"{}","timestamp":"{}"}}"#,
                    self.level,
                    self.target,
                    self.message,
                    self.timestamp.elapsed().as_millis()
                )
            },
            LogFormat::Text => {
                format!(
                    "[{}] {} {}: {}",
                    self.timestamp.elapsed().as_millis(),
                    self.level,
                    self.target,
                    self.message
                )
            },
            LogFormat::Binary => {
                // Binary format - for now we'll use a compact text representation
                // In a real implementation this would be a proper binary serialization
                format!(
                    "{}|{}|{}|{}",
                    self.level,
                    self.target,
                    self.message,
                    self.timestamp.elapsed().as_millis()
                )
            },
        }
    }
}

/// Metrics for the logging system
#[derive(Debug, Default)]
pub struct LoggingMetrics {
    /// Total number of log entries processed
    pub entries_processed: AtomicU64,
    /// Number of entries dropped due to buffer overflow
    pub entries_dropped: AtomicU64,
    /// Number of formatting errors
    pub formatting_errors: AtomicU64,
    /// Total bytes written to logs
    pub bytes_written: AtomicU64,
    /// Number of flush operations
    pub flush_operations: AtomicU64,
}

impl LoggingMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Get snapshot of current metrics
    pub fn snapshot(&self) -> LoggingMetricsSnapshot {
        LoggingMetricsSnapshot {
            entries_processed: self.entries_processed.load(Ordering::Relaxed),
            entries_dropped: self.entries_dropped.load(Ordering::Relaxed),
            formatting_errors: self.formatting_errors.load(Ordering::Relaxed),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
            flush_operations: self.flush_operations.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of logging metrics
#[derive(Debug, Clone)]
pub struct LoggingMetricsSnapshot {
    pub entries_processed: u64,
    pub entries_dropped: u64,
    pub formatting_errors: u64,
    pub bytes_written: u64,
    pub flush_operations: u64,
}

impl LoggingSystem {
    /// Create a new logging system instance
    pub fn new(config: LoggingConfig) -> Result<Self> {
        let (sender, _receiver) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            sender,
            metrics: Arc::new(LoggingMetrics::new()),
            health: Arc::new(RwLock::new(HealthStatus::Healthy)),
        })
    }

    /// Log a message at the specified level
    pub fn log(&self, level: LogLevel, target: String, message: String) -> Result<()> {
        self.log_with_fields(level, target, message, HashMap::new())
    }

    /// Log a message with structured fields
    pub fn log_with_fields(
        &self,
        level: LogLevel,
        target: String,
        message: String,
        fields: HashMap<String, String>,
    ) -> Result<()> {
        // Check if logging is enabled and level meets threshold
        if !self.config.enabled || !self.should_log(&level) {
            return Ok(());
        }

        let entry = LogEntry::new(level, target, message).with_fields(fields);

        // Try to send the entry
        if let Err(_) = self.sender.send(entry) {
            self.metrics.entries_dropped.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.entries_processed.fetch_add(1, Ordering::Relaxed);
        }

        Ok(())
    }

    /// Check if a log level should be processed
    fn should_log(&self, level: &LogLevel) -> bool {
        use LogLevel::*;

        let level_priority = match level {
            Error => 5,
            Warn => 4,
            Info => 3,
            Debug => 2,
            Trace => 1,
        };

        let config_priority = match self.config.level {
            Error => 5,
            Warn => 4,
            Info => 3,
            Debug => 2,
            Trace => 1,
        };

        level_priority >= config_priority
    }

    /// Get current metrics snapshot
    pub fn metrics(&self) -> LoggingMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Flush any buffered log entries
    pub async fn flush(&self) -> Result<()> {
        self.metrics.flush_operations.fetch_add(1, Ordering::Relaxed);
        // Implementation would flush buffers here
        Ok(())
    }

    /// Shutdown the logging system gracefully
    pub async fn shutdown(&self) -> Result<()> {
        self.flush().await?;
        Ok(())
    }
}

impl HealthIndicator for LoggingSystem {
    fn name(&self) -> &str {
        "logging_system"
    }

    async fn check_health(&self) -> HealthStatus {
        let metrics = self.metrics();

        // Consider unhealthy if too many entries are being dropped
        if metrics.entries_dropped > 0 && metrics.entries_processed > 0 {
            let drop_rate = metrics.entries_dropped as f64 / metrics.entries_processed as f64;
            if drop_rate > 0.1 {
                // More than 10% drop rate
                return HealthStatus::Unhealthy("High log drop rate".to_string());
            }
        }

        HealthStatus::Healthy
    }

    fn dependencies(&self) -> Vec<String> {
        Vec::new() // Logging system has no dependencies
    }

    fn health_status(&self) -> HealthStatus {
        // Return the current health status synchronously
        // In a real implementation, this might read from a cached status
        HealthStatus::Healthy
    }
}

// Display implementations for LogLevel
impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(LogLevel::Info, "test".to_string(), "Test message".to_string());

        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.target, "test");
        assert_eq!(entry.message, "Test message");
        assert!(entry.fields.is_empty());
    }

    #[test]
    fn test_log_entry_with_fields() {
        let mut fields = HashMap::new();
        fields.insert("key1".to_string(), "value1".to_string());
        fields.insert("key2".to_string(), "value2".to_string());

        let entry = LogEntry::new(LogLevel::Error, "test".to_string(), "Error message".to_string())
            .with_fields(fields.clone());

        assert_eq!(entry.fields, fields);
    }

    #[test]
    fn test_json_formatting() {
        let entry = LogEntry::new(LogLevel::Info, "test".to_string(), "Test message".to_string());

        let formatted = entry.format(&LogFormat::Json);
        assert!(formatted.contains("\"level\":\"INFO\""));
        assert!(formatted.contains("\"target\":\"test\""));
        assert!(formatted.contains("\"message\":\"Test message\""));
    }

    #[test]
    fn test_text_formatting() {
        let entry =
            LogEntry::new(LogLevel::Warn, "test".to_string(), "Warning message".to_string());

        let formatted = entry.format(&LogFormat::Text);
        assert!(formatted.contains("WARN"));
        assert!(formatted.contains("test:"));
        assert!(formatted.contains("Warning message"));
    }

    #[test]
    fn test_binary_formatting() {
        let entry = LogEntry::new(LogLevel::Debug, "test".to_string(), "Debug message".to_string());

        let formatted = entry.format(&LogFormat::Binary);
        assert!(formatted.contains("DEBUG"));
        assert!(formatted.contains("test"));
        assert!(formatted.contains("Debug message"));
        // Binary format uses pipe separator
        assert!(formatted.contains("|"));
    }

    #[tokio::test]
    async fn test_logging_system_creation() {
        let config = LoggingConfig::default();
        let system = LoggingSystem::new(config);
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_logging_with_different_levels() {
        let config = LoggingConfig { enabled: true, level: LogLevel::Info, ..Default::default() };

        let system = LoggingSystem::new(config).unwrap();

        // These should be logged (Info level and above)
        system
            .log(LogLevel::Info, "test".to_string(), "Info message".to_string())
            .unwrap();
        system
            .log(LogLevel::Warn, "test".to_string(), "Warn message".to_string())
            .unwrap();
        system
            .log(LogLevel::Error, "test".to_string(), "Error message".to_string())
            .unwrap();

        // This should not be logged (below Info level)
        system
            .log(LogLevel::Debug, "test".to_string(), "Debug message".to_string())
            .unwrap();

        let metrics = system.metrics();
        assert_eq!(metrics.entries_processed, 3); // Only Info, Warn, and Error should be processed
    }

    #[tokio::test]
    async fn test_health_indicator() {
        let config = LoggingConfig::default();
        let system = LoggingSystem::new(config).unwrap();

        let health = system.check_health().await;
        assert!(matches!(health, HealthStatus::Healthy));

        assert_eq!(system.name(), "logging_system");
        assert!(system.dependencies().is_empty());
    }
}
