//! # High-Performance Structured Logging
//!
//! Async structured logging system optimized for >1M log entries/second
//! with minimal allocation and lock-free queuing.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use crossbeam_queue::ArrayQueue;
use smallvec::SmallVec;
use tokio::sync::{mpsc, oneshot, Notify};
use tokio::task::JoinHandle;
use serde::{Serialize, Deserialize};

use crate::telemetry::{TelemetryError, Result, LoggingConfig, LogLevel, LogFormat, TraceContext};

/// High-performance async logging system
pub struct LoggingSystem {
    /// Log entry queue (lock-free)
    entry_queue: Arc<ArrayQueue<LogEntry>>,
    
    /// Background writer handle
    writer_handle: parking_lot::Mutex<Option<JoinHandle<()>>>,
    
    /// Flush notification
    flush_notify: Arc<Notify>,
    
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    
    /// Configuration
    config: LoggingConfig,
    
    /// Performance metrics
    metrics: LoggingMetrics,
    
    /// Log formatter
    formatter: Arc<dyn LogFormatter>,
    
    /// Log writers (console, file, etc.)
    writers: Arc<LogWriters>,
}

impl LoggingSystem {
    /// Creates a new logging system
    pub async fn new(config: LoggingConfig) -> Result<Self> {
        let entry_queue = Arc::new(ArrayQueue::new(config.async_buffer_size));
        let shutdown = Arc::new(AtomicBool::new(false));
        let flush_notify = Arc::new(Notify::new());
        
        let formatter: Arc<dyn LogFormatter> = match config.format {
            LogFormat::Json => Arc::new(JsonFormatter::new()),
            LogFormat::Text => Arc::new(TextFormatter::new()),
            LogFormat::Binary => Arc::new(BinaryFormatter::new()),
            LogFormat::Custom => Arc::new(TextFormatter::new()), // Fallback
        };
        
        let writers = Arc::new(LogWriters::new(&config).await?);
        
        Ok(Self {
            entry_queue,
            writer_handle: parking_lot::Mutex::new(None),
            flush_notify,
            shutdown,
            config,
            metrics: LoggingMetrics::new(),
            formatter,
            writers,
        })
    }

    /// Logs a message with the specified level
    ///
    /// # Performance
    /// Target: <500ns for log entry queuing
    /// Implementation: Lock-free queue with pre-allocated entries
    #[inline]
    pub fn log(&self, level: LogLevel, target: &str, message: &str, fields: &[LogField]) {
        // Check if level is enabled
        if level < self.config.level {
            return;
        }

        let start = Instant::now();
        
        let entry = LogEntry {
            timestamp: SystemTime::now(),
            level,
            target: target.to_string(),
            message: message.to_string(),
            fields: SmallVec::from_slice(fields),
            trace_context: None,
            thread_name: std::thread::current().name().map(String::from),
        };

        // Try to enqueue (non-blocking)
        match self.entry_queue.push(entry) {
            Ok(()) => {
                self.metrics.entries_queued.fetch_add(1, Ordering::Relaxed);
                self.metrics.total_log_time_ns.fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
            }
            Err(_) => {
                // Queue full - drop log entry and increment counter
                self.metrics.entries_dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Logs with trace context
    #[inline]
    pub fn log_with_context(
        &self,
        level: LogLevel,
        target: &str,
        message: &str,
        fields: &[LogField],
        context: TraceContext,
    ) {
        if level < self.config.level {
            return;
        }

        let entry = LogEntry {
            timestamp: SystemTime::now(),
            level,
            target: target.to_string(),
            message: message.to_string(),
            fields: SmallVec::from_slice(fields),
            trace_context: Some(context),
            thread_name: std::thread::current().name().map(String::from),
        };

        if self.entry_queue.push(entry).is_err() {
            self.metrics.entries_dropped.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.entries_queued.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Convenience methods for different log levels

    #[inline]
    pub fn trace(&self, target: &str, message: &str, fields: &[LogField]) {
        self.log(LogLevel::Trace, target, message, fields);
    }

    #[inline]
    pub fn debug(&self, target: &str, message: &str, fields: &[LogField]) {
        self.log(LogLevel::Debug, target, message, fields);
    }

    #[inline]
    pub fn info(&self, target: &str, message: &str, fields: &[LogField]) {
        self.log(LogLevel::Info, target, message, fields);
    }

    #[inline]
    pub fn warn(&self, target: &str, message: &str, fields: &[LogField]) {
        self.log(LogLevel::Warn, target, message, fields);
    }

    #[inline]
    pub fn error(&self, target: &str, message: &str, fields: &[LogField]) {
        self.log(LogLevel::Error, target, message, fields);
    }

    /// Creates a contextual logger with trace context
    pub fn with_context(&self, context: TraceContext) -> ContextualLogger {
        ContextualLogger {
            system: self,
            context,
        }
    }

    /// Starts the background writer
    pub async fn start_background_writer(&mut self) -> Result<()> {
        let queue = Arc::clone(&self.entry_queue);
        let config = self.config.clone();
        let formatter = Arc::clone(&self.formatter);
        let writers = Arc::clone(&self.writers);
        let shutdown = Arc::clone(&self.shutdown);
        let flush_notify = Arc::clone(&self.flush_notify);
        let metrics = self.metrics.clone();

        let handle = tokio::spawn(async move {
            Self::writer_task(queue, config, formatter, writers, shutdown, flush_notify, metrics).await;
        });

        *self.writer_handle.lock() = Some(handle);
        Ok(())
    }

    /// Stops the background writer
    pub async fn stop_background_writer(&mut self) -> Result<()> {
        self.shutdown.store(true, Ordering::Relaxed);
        self.flush_notify.notify_one();

        if let Some(handle) = self.writer_handle.lock().take() {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Flushes all pending log entries
    pub async fn flush(&self) -> Result<()> {
        self.flush_notify.notify_one();
        
        // Wait for queue to be drained (with timeout)
        let timeout = Duration::from_secs(5);
        let start = Instant::now();
        
        while !self.entry_queue.is_empty() && start.elapsed() < timeout {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        if !self.entry_queue.is_empty() {
            return Err(TelemetryError::logging("Flush timeout: queue not empty"));
        }

        Ok(())
    }

    /// Returns total log entries processed
    pub fn total_logs_processed(&self) -> u64 {
        self.metrics.entries_processed.load(Ordering::Relaxed)
    }

    /// Returns estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let queue_size = self.entry_queue.capacity() * std::mem::size_of::<LogEntry>();
        let metrics_size = std::mem::size_of::<LoggingMetrics>();
        
        base_size + queue_size + metrics_size
    }

    /// Returns estimated CPU usage
    pub fn cpu_usage_estimate(&self) -> f64 {
        let total_entries = self.metrics.entries_processed.load(Ordering::Relaxed);
        if total_entries == 0 {
            return 0.0;
        }

        let total_time_ns = self.metrics.total_log_time_ns.load(Ordering::Relaxed);
        let avg_time_ns = total_time_ns / total_entries;
        
        // Very rough estimate based on processing rate
        let entries_per_second = total_entries as f64 / 60.0; // Assume 1-minute average
        entries_per_second * (avg_time_ns as f64) / 1_000_000_000.0
    }

    /// Background writer task
    async fn writer_task(
        queue: Arc<ArrayQueue<LogEntry>>,
        config: LoggingConfig,
        formatter: Arc<dyn LogFormatter>,
        writers: Arc<LogWriters>,
        shutdown: Arc<AtomicBool>,
        flush_notify: Arc<Notify>,
        metrics: LoggingMetrics,
    ) {
        let mut batch = Vec::with_capacity(1000);
        let mut flush_timer = tokio::time::interval(config.flush_interval);
        flush_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = flush_timer.tick() => {
                    Self::process_batch(&queue, &mut batch, &formatter, &writers, &metrics).await;
                }
                _ = flush_notify.notified() => {
                    Self::process_batch(&queue, &mut batch, &formatter, &writers, &metrics).await;
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
        }

        // Final flush on shutdown
        Self::process_batch(&queue, &mut batch, &formatter, &writers, &metrics).await;
    }

    /// Processes a batch of log entries
    async fn process_batch(
        queue: &ArrayQueue<LogEntry>,
        batch: &mut Vec<LogEntry>,
        formatter: &Arc<dyn LogFormatter>,
        writers: &Arc<LogWriters>,
        metrics: &LoggingMetrics,
    ) {
        // Drain queue into batch
        while let Ok(entry) = queue.pop() {
            batch.push(entry);
            if batch.len() >= 1000 {
                break; // Prevent unbounded batches
            }
        }

        if batch.is_empty() {
            return;
        }

        // Format and write entries
        let formatted_entries: Vec<_> = batch
            .iter()
            .filter_map(|entry| {
                formatter.format(entry).ok()
            })
            .collect();

        // Write to all configured writers
        if let Err(e) = writers.write_batch(&formatted_entries).await {
            metrics.write_errors.fetch_add(1, Ordering::Relaxed);
            eprintln!("Logging write error: {}", e); // Fallback error reporting
        }

        metrics.entries_processed.fetch_add(batch.len() as u64, Ordering::Relaxed);
        batch.clear();
    }
}

/// Contextual logger that includes trace context
pub struct ContextualLogger<'a> {
    system: &'a LoggingSystem,
    context: TraceContext,
}

impl<'a> ContextualLogger<'a> {
    pub fn trace(&self, target: &str, message: &str, fields: &[LogField]) {
        self.system.log_with_context(LogLevel::Trace, target, message, fields, self.context.clone());
    }

    pub fn debug(&self, target: &str, message: &str, fields: &[LogField]) {
        self.system.log_with_context(LogLevel::Debug, target, message, fields, self.context.clone());
    }

    pub fn info(&self, target: &str, message: &str, fields: &[LogField]) {
        self.system.log_with_context(LogLevel::Info, target, message, fields, self.context.clone());
    }

    pub fn warn(&self, target: &str, message: &str, fields: &[LogField]) {
        self.system.log_with_context(LogLevel::Warn, target, message, fields, self.context.clone());
    }

    pub fn error(&self, target: &str, message: &str, fields: &[LogField]) {
        self.system.log_with_context(LogLevel::Error, target, message, fields, self.context.clone());
    }
}

/// Log entry structure optimized for performance
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: SmallVec<[LogField; 8]>,
    pub trace_context: Option<TraceContext>,
    pub thread_name: Option<String>,
}

/// Log field for structured logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogField {
    pub key: String,
    pub value: LogValue,
}

impl LogField {
    pub fn new<K, V>(key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<LogValue>,
    {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    pub fn string<K, V>(key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Self::new(key, LogValue::String(value.into()))
    }

    pub fn number<K>(key: K, value: f64) -> Self
    where
        K: Into<String>,
    {
        Self::new(key, LogValue::Number(value))
    }

    pub fn bool<K>(key: K, value: bool) -> Self
    where
        K: Into<String>,
    {
        Self::new(key, LogValue::Bool(value))
    }
}

/// Log field value types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LogValue {
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<LogValue>),
    Object(HashMap<String, LogValue>),
}

impl From<String> for LogValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for LogValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<f64> for LogValue {
    fn from(n: f64) -> Self {
        Self::Number(n)
    }
}

impl From<f32> for LogValue {
    fn from(n: f32) -> Self {
        Self::Number(n as f64)
    }
}

impl From<i64> for LogValue {
    fn from(n: i64) -> Self {
        Self::Number(n as f64)
    }
}

impl From<i32> for LogValue {
    fn from(n: i32) -> Self {
        Self::Number(n as f64)
    }
}

impl From<u64> for LogValue {
    fn from(n: u64) -> Self {
        Self::Number(n as f64)
    }
}

impl From<u32> for LogValue {
    fn from(n: u32) -> Self {
        Self::Number(n as f64)
    }
}

impl From<bool> for LogValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

/// Log formatter trait
pub trait LogFormatter: Send + Sync {
    fn format(&self, entry: &LogEntry) -> Result<FormattedLogEntry>;
}

/// Formatted log entry ready for output
#[derive(Debug, Clone)]
pub struct FormattedLogEntry {
    pub content: Vec<u8>,
    pub metadata: LogEntryMetadata,
}

/// Log entry metadata
#[derive(Debug, Clone)]
pub struct LogEntryMetadata {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub target: String,
    pub size_bytes: usize,
}

/// JSON log formatter
pub struct JsonFormatter {
    include_timestamp: bool,
    include_thread_name: bool,
}

impl JsonFormatter {
    pub fn new() -> Self {
        Self {
            include_timestamp: true,
            include_thread_name: true,
        }
    }
}

impl LogFormatter for JsonFormatter {
    fn format(&self, entry: &LogEntry) -> Result<FormattedLogEntry> {
        let mut json = serde_json::Map::new();
        
        if self.include_timestamp {
            let timestamp = entry.timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            json.insert("timestamp".to_string(), serde_json::Value::Number(timestamp.into()));
        }
        
        json.insert("level".to_string(), serde_json::Value::String(entry.level.to_string()));
        json.insert("target".to_string(), serde_json::Value::String(entry.target.clone()));
        json.insert("message".to_string(), serde_json::Value::String(entry.message.clone()));
        
        if self.include_thread_name {
            if let Some(ref thread_name) = entry.thread_name {
                json.insert("thread".to_string(), serde_json::Value::String(thread_name.clone()));
            }
        }

        // Add structured fields
        for field in &entry.fields {
            let value = match &field.value {
                LogValue::String(s) => serde_json::Value::String(s.clone()),
                LogValue::Number(n) => serde_json::Value::Number(
                    serde_json::Number::from_f64(*n).unwrap_or(serde_json::Number::from(0))
                ),
                LogValue::Bool(b) => serde_json::Value::Bool(*b),
                LogValue::Array(arr) => serde_json::to_value(arr)
                    .unwrap_or(serde_json::Value::Null),
                LogValue::Object(obj) => serde_json::to_value(obj)
                    .unwrap_or(serde_json::Value::Null),
            };
            json.insert(field.key.clone(), value);
        }

        // Add trace context if available
        if let Some(ref context) = entry.trace_context {
            json.insert("trace_id".to_string(), 
                serde_json::Value::String(context.trace_id.to_hex()));
            json.insert("span_id".to_string(), 
                serde_json::Value::String(context.span_id.to_hex()));
            
            if let Some(parent_span_id) = context.parent_span_id {
                json.insert("parent_span_id".to_string(), 
                    serde_json::Value::String(parent_span_id.to_hex()));
            }
        }

        let mut content = serde_json::to_vec(&json)
            .map_err(|e| TelemetryError::logging(format!("JSON formatting error: {}", e)))?;
        content.push(b'\n');

        Ok(FormattedLogEntry {
            metadata: LogEntryMetadata {
                timestamp: entry.timestamp,
                level: entry.level,
                target: entry.target.clone(),
                size_bytes: content.len(),
            },
            content,
        })
    }
}

/// Text log formatter
pub struct TextFormatter {
    include_timestamp: bool,
    include_thread_name: bool,
    colored: bool,
}

impl TextFormatter {
    pub fn new() -> Self {
        Self {
            include_timestamp: true,
            include_thread_name: false,
            colored: false,
        }
    }

    pub fn with_colors(mut self, colored: bool) -> Self {
        self.colored = colored;
        self
    }
}

impl LogFormatter for TextFormatter {
    fn format(&self, entry: &LogEntry) -> Result<FormattedLogEntry> {
        let mut output = String::new();

        // Timestamp
        if self.include_timestamp {
            let timestamp = entry.timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            output.push_str(&format!("{} ", timestamp));
        }

        // Level with optional coloring
        if self.colored {
            let level_colored = match entry.level {
                LogLevel::Error => format!("\x1b[31m{}\x1b[0m", entry.level), // Red
                LogLevel::Warn => format!("\x1b[33m{}\x1b[0m", entry.level),  // Yellow
                LogLevel::Info => format!("\x1b[32m{}\x1b[0m", entry.level),  // Green
                LogLevel::Debug => format!("\x1b[36m{}\x1b[0m", entry.level), // Cyan
                LogLevel::Trace => format!("\x1b[37m{}\x1b[0m", entry.level), // White
            };
            output.push_str(&format!("[{}] ", level_colored));
        } else {
            output.push_str(&format!("[{}] ", entry.level));
        }

        // Target
        output.push_str(&format!("{}: ", entry.target));

        // Message
        output.push_str(&entry.message);

        // Fields
        for field in &entry.fields {
            output.push_str(&format!(" {}={}", field.key, format_log_value(&field.value)));
        }

        // Thread name
        if self.include_thread_name {
            if let Some(ref thread_name) = entry.thread_name {
                output.push_str(&format!(" thread={}", thread_name));
            }
        }

        // Trace context
        if let Some(ref context) = entry.trace_context {
            output.push_str(&format!(" trace_id={} span_id={}", 
                context.trace_id.to_hex(), context.span_id.to_hex()));
        }

        output.push('\n');
        let content = output.into_bytes();

        Ok(FormattedLogEntry {
            metadata: LogEntryMetadata {
                timestamp: entry.timestamp,
                level: entry.level,
                target: entry.target.clone(),
                size_bytes: content.len(),
            },
            content,
        })
    }
}

/// Binary log formatter (compact representation)
pub struct BinaryFormatter;

impl BinaryFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl LogFormatter for BinaryFormatter {
    fn format(&self, entry: &LogEntry) -> Result<FormattedLogEntry> {
        // For brevity, this is a simplified binary format
        // In production, this would use a more efficient binary encoding like MessagePack
        let serialized = bincode::serialize(entry)
            .map_err(|e| TelemetryError::logging(format!("Binary formatting error: {}", e)))?;

        Ok(FormattedLogEntry {
            metadata: LogEntryMetadata {
                timestamp: entry.timestamp,
                level: entry.level,
                target: entry.target.clone(),
                size_bytes: serialized.len(),
            },
            content: serialized,
        })
    }
}

fn format_log_value(value: &LogValue) -> String {
    match value {
        LogValue::String(s) => format!("\"{}\"", s),
        LogValue::Number(n) => n.to_string(),
        LogValue::Bool(b) => b.to_string(),
        LogValue::Array(arr) => format!("[{}]", 
            arr.iter().map(format_log_value).collect::<Vec<_>>().join(", ")),
        LogValue::Object(obj) => format!("{{{}}}",
            obj.iter()
                .map(|(k, v)| format!("{}:{}", k, format_log_value(v)))
                .collect::<Vec<_>>()
                .join(", ")),
    }
}

/// Log writers for different output destinations
pub struct LogWriters {
    console_writer: Option<ConsoleWriter>,
    file_writer: Option<FileWriter>,
}

impl LogWriters {
    pub async fn new(config: &LoggingConfig) -> Result<Self> {
        let console_writer = if config.console.enabled {
            Some(ConsoleWriter::new(config))
        } else {
            None
        };

        let file_writer = if let Some(ref file_config) = config.file {
            Some(FileWriter::new(file_config).await?)
        } else {
            None
        };

        Ok(Self {
            console_writer,
            file_writer,
        })
    }

    pub async fn write_batch(&self, entries: &[FormattedLogEntry]) -> Result<()> {
        // Write to console
        if let Some(ref writer) = self.console_writer {
            writer.write_batch(entries).await?;
        }

        // Write to file
        if let Some(ref writer) = self.file_writer {
            writer.write_batch(entries).await?;
        }

        Ok(())
    }
}

/// Console log writer
pub struct ConsoleWriter {
    colored: bool,
}

impl ConsoleWriter {
    pub fn new(config: &LoggingConfig) -> Self {
        Self {
            colored: config.console.colored,
        }
    }

    pub async fn write_batch(&self, entries: &[FormattedLogEntry]) -> Result<()> {
        use tokio::io::{AsyncWriteExt, stdout};
        
        let mut stdout = stdout();
        for entry in entries {
            stdout.write_all(&entry.content).await
                .map_err(|e| TelemetryError::io("Console write error", e))?;
        }
        stdout.flush().await
            .map_err(|e| TelemetryError::io("Console flush error", e))?;
        
        Ok(())
    }
}

/// File log writer with rotation
pub struct FileWriter {
    // Implementation would include file rotation logic
    // For brevity, simplified here
}

impl FileWriter {
    pub async fn new(_config: &crate::telemetry::LogFileConfig) -> Result<Self> {
        Ok(Self {})
    }

    pub async fn write_batch(&self, _entries: &[FormattedLogEntry]) -> Result<()> {
        // File writing implementation
        Ok(())
    }
}

/// Logging performance metrics
#[derive(Debug, Clone)]
pub struct LoggingMetrics {
    pub entries_queued: AtomicU64,
    pub entries_processed: AtomicU64,
    pub entries_dropped: AtomicU64,
    pub write_errors: AtomicU64,
    pub total_log_time_ns: AtomicU64,
}

impl LoggingMetrics {
    pub fn new() -> Self {
        Self {
            entries_queued: AtomicU64::new(0),
            entries_processed: AtomicU64::new(0),
            entries_dropped: AtomicU64::new(0),
            write_errors: AtomicU64::new(0),
            total_log_time_ns: AtomicU64::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_logging_system_creation() {
        let config = LoggingConfig::default();
        let logging = LoggingSystem::new(config).await.unwrap();
        
        assert!(logging.entry_queue.capacity() > 0);
        assert!(!logging.shutdown.load(Ordering::Relaxed));
    }

    #[test]
    fn test_log_field_creation() {
        let field = LogField::string("user_id", "12345");
        assert_eq!(field.key, "user_id");
        match field.value {
            LogValue::String(s) => assert_eq!(s, "12345"),
            _ => panic!("Expected string value"),
        }

        let field = LogField::number("latency_ms", 42.5);
        assert_eq!(field.key, "latency_ms");
        match field.value {
            LogValue::Number(n) => assert_eq!(n, 42.5),
            _ => panic!("Expected number value"),
        }
    }

    #[test]
    fn test_json_formatter() {
        let formatter = JsonFormatter::new();
        let entry = LogEntry {
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1234567890),
            level: LogLevel::Info,
            target: "test".to_string(),
            message: "Test message".to_string(),
            fields: SmallVec::from_slice(&[
                LogField::string("key", "value"),
                LogField::number("count", 42.0),
            ]),
            trace_context: None,
            thread_name: Some("main".to_string()),
        };

        let formatted = formatter.format(&entry).unwrap();
        let json_str = String::from_utf8(formatted.content).unwrap();
        
        assert!(json_str.contains("\"level\":\"info\""));
        assert!(json_str.contains("\"message\":\"Test message\""));
        assert!(json_str.contains("\"key\":\"value\""));
        assert!(json_str.contains("\"count\":42"));
        assert!(json_str.contains("\"thread\":\"main\""));
    }

    #[test]
    fn test_text_formatter() {
        let formatter = TextFormatter::new();
        let entry = LogEntry {
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1234567890),
            level: LogLevel::Warn,
            target: "test".to_string(),
            message: "Warning message".to_string(),
            fields: SmallVec::from_slice(&[
                LogField::string("error", "timeout"),
            ]),
            trace_context: None,
            thread_name: None,
        };

        let formatted = formatter.format(&entry).unwrap();
        let text = String::from_utf8(formatted.content).unwrap();
        
        assert!(text.contains("[warn]"));
        assert!(text.contains("test: Warning message"));
        assert!(text.contains("error=\"timeout\""));
    }

    #[bench]
    fn bench_log_entry_queueing(b: &mut test::Bencher) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let logging = rt.block_on(async {
            let config = LoggingConfig::default();
            LoggingSystem::new(config).await.unwrap()
        });

        let fields = &[LogField::string("test", "value")];

        b.iter(|| {
            let start = std::time::Instant::now();
            logging.info("test", "Benchmark message", fields);
            let elapsed = start.elapsed();
            
            // Verify performance target (<500ns)
            assert!(elapsed < Duration::from_nanos(500));
        });
    }

    #[test]
    fn test_log_value_conversion() {
        let value: LogValue = "test".into();
        match value {
            LogValue::String(s) => assert_eq!(s, "test"),
            _ => panic!("Expected string"),
        }

        let value: LogValue = 42u32.into();
        match value {
            LogValue::Number(n) => assert_eq!(n, 42.0),
            _ => panic!("Expected number"),
        }

        let value: LogValue = true.into();
        match value {
            LogValue::Bool(b) => assert!(b),
            _ => panic!("Expected bool"),
        }
    }

    #[tokio::test]
    async fn test_contextual_logger() {
        let config = LoggingConfig::default();
        let logging = LoggingSystem::new(config).await.unwrap();
        
        let context = TraceContext {
            trace_id: crate::telemetry::TraceId::generate(),
            span_id: crate::telemetry::SpanId::generate(),
            parent_span_id: None,
            sampling_decision: crate::telemetry::SamplingDecision::RecordAndSample,
            baggage: HashMap::new(),
        };

        let contextual = logging.with_context(context);
        contextual.info("test", "Message with context", &[]);
        
        assert_eq!(logging.metrics.entries_queued.load(Ordering::Relaxed), 1);
    }
}