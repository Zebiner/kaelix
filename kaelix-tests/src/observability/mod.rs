//! # Observability Utilities
//!
//! Comprehensive observability infrastructure for test execution including distributed tracing,
//! structured logging, system information collection, and baseline performance benchmarking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{debug, info, span, warn, Level, Span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Test tracer for distributed tracing and span management
#[derive(Debug, Clone)]
pub struct TestTracer {
    /// OpenTelemetry tracer (simplified for this implementation)
    tracer_name: String,
    /// Span storage for test correlation
    active_spans: Arc<parking_lot::RwLock<HashMap<String, SpanInfo>>>,
    /// Trace context storage
    trace_context: Arc<parking_lot::RwLock<Option<TraceContext>>>,
}

/// Span information for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanInfo {
    /// Span ID
    pub span_id: String,
    /// Span name
    pub name: String,
    /// Start time
    pub start_time: SystemTime,
    /// End time (if completed)
    pub end_time: Option<SystemTime>,
    /// Span attributes
    pub attributes: HashMap<String, String>,
    /// Parent span ID
    pub parent_id: Option<String>,
    /// Span status
    pub status: SpanStatus,
}

/// Span status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpanStatus {
    /// Span is active
    Active,
    /// Span completed successfully
    Ok,
    /// Span completed with error
    Error(String),
    /// Span was cancelled
    Cancelled,
}

/// Trace context for correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Trace ID
    pub trace_id: String,
    /// Root span ID
    pub root_span_id: String,
    /// Test suite name
    pub suite_name: String,
    /// Test name
    pub test_name: Option<String>,
    /// Additional context
    pub context: HashMap<String, String>,
}

/// Event for tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event name
    pub name: String,
    /// Event timestamp
    pub timestamp: SystemTime,
    /// Event level
    pub level: EventLevel,
    /// Event attributes
    pub attributes: HashMap<String, String>,
    /// Event message
    pub message: String,
    /// Span ID this event belongs to
    pub span_id: Option<String>,
}

/// Event level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Trace data for export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    /// Trace ID
    pub trace_id: String,
    /// All spans in the trace
    pub spans: Vec<SpanInfo>,
    /// All events in the trace
    pub events: Vec<Event>,
    /// Trace duration
    pub duration: Duration,
    /// Trace metadata
    pub metadata: HashMap<String, String>,
}

impl TestTracer {
    /// Creates a new test tracer
    pub fn new() -> Self {
        Self {
            tracer_name: "memorystreamer-tests".to_string(),
            active_spans: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            trace_context: Arc::new(parking_lot::RwLock::new(None)),
        }
    }
    
    /// Starts a new trace context
    pub fn start_trace(&self, suite_name: &str, test_name: Option<&str>) -> String {
        let trace_id = format!("trace_{}", uuid::Uuid::new_v4());
        let root_span_id = format!("span_{}", uuid::Uuid::new_v4());
        
        let context = TraceContext {
            trace_id: trace_id.clone(),
            root_span_id: root_span_id.clone(),
            suite_name: suite_name.to_string(),
            test_name: test_name.map(|s| s.to_string()),
            context: HashMap::new(),
        };
        
        *self.trace_context.write() = Some(context);
        
        // Create root span
        let root_span = SpanInfo {
            span_id: root_span_id.clone(),
            name: format!("test_{}", suite_name),
            start_time: SystemTime::now(),
            end_time: None,
            attributes: HashMap::new(),
            parent_id: None,
            status: SpanStatus::Active,
        };
        
        self.active_spans.write().insert(root_span_id.clone(), root_span);
        
        info!("Started trace: {} for suite: {}", trace_id, suite_name);
        trace_id
    }
    
    /// Starts a new span
    pub fn start_span(&self, name: &str) -> TestSpan {
        let span_id = format!("span_{}", uuid::Uuid::new_v4());
        
        let parent_id = self.trace_context.read()
            .as_ref()
            .map(|ctx| ctx.root_span_id.clone());
        
        let span_info = SpanInfo {
            span_id: span_id.clone(),
            name: name.to_string(),
            start_time: SystemTime::now(),
            end_time: None,
            attributes: HashMap::new(),
            parent_id,
            status: SpanStatus::Active,
        };
        
        self.active_spans.write().insert(span_id.clone(), span_info);
        
        debug!("Started span: {} ({})", span_id, name);
        
        TestSpan::new(span_id, name.to_string(), self.clone())
    }
    
    /// Records an event
    pub fn record_event(&self, event: &Event) {
        debug!("Recording event: {} at {:?}", event.name, event.timestamp);
        
        // In a real implementation, this would send to OpenTelemetry collector
        // For now, we'll just log it
    }
    
    /// Completes a span
    pub fn complete_span(&self, span_id: &str, status: SpanStatus) {
        if let Some(mut span) = self.active_spans.write().get_mut(span_id) {
            span.end_time = Some(SystemTime::now());
            span.status = status;
            
            debug!("Completed span: {}", span_id);
        }
    }
    
    /// Exports all traces
    pub fn export_traces(&self) -> Vec<Trace> {
        let spans = self.active_spans.read();
        let context = self.trace_context.read();
        
        if let Some(ctx) = context.as_ref() {
            let trace_spans: Vec<SpanInfo> = spans.values().cloned().collect();
            
            let duration = if let (Some(first), Some(last)) = (
                trace_spans.iter().map(|s| s.start_time).min(),
                trace_spans.iter().filter_map(|s| s.end_time).max()
            ) {
                last.duration_since(first).unwrap_or_default()
            } else {
                Duration::ZERO
            };
            
            vec![Trace {
                trace_id: ctx.trace_id.clone(),
                spans: trace_spans,
                events: Vec::new(), // Would collect events in real implementation
                duration,
                metadata: ctx.context.clone(),
            }]
        } else {
            Vec::new()
        }
    }
    
    /// Clears all trace data
    pub fn clear(&self) {
        self.active_spans.write().clear();
        *self.trace_context.write() = None;
        info!("Cleared all trace data");
    }
}

impl Default for TestTracer {
    fn default() -> Self {
        Self::new()
    }
}

/// Test span wrapper for automatic cleanup
#[derive(Debug)]
pub struct TestSpan {
    span_id: String,
    name: String,
    tracer: TestTracer,
    _span: Span, // Tracing span for integration
}

impl TestSpan {
    fn new(span_id: String, name: String, tracer: TestTracer) -> Self {
        let span = span!(Level::INFO, "test_span", span_id = %span_id, name = %name);
        
        Self {
            span_id,
            name,
            tracer,
            _span: span,
        }
    }
    
    /// Adds an attribute to the span
    pub fn set_attribute(&self, key: &str, value: &str) {
        if let Some(mut span) = self.tracer.active_spans.write().get_mut(&self.span_id) {
            span.attributes.insert(key.to_string(), value.to_string());
        }
    }
    
    /// Records an event on this span
    pub fn record_event(&self, name: &str, message: &str) {
        let event = Event {
            name: name.to_string(),
            timestamp: SystemTime::now(),
            level: EventLevel::Info,
            attributes: HashMap::new(),
            message: message.to_string(),
            span_id: Some(self.span_id.clone()),
        };
        
        self.tracer.record_event(&event);
    }
    
    /// Marks the span as successful
    pub fn success(self) {
        self.tracer.complete_span(&self.span_id, SpanStatus::Ok);
    }
    
    /// Marks the span as failed
    pub fn error(self, error: &str) {
        self.tracer.complete_span(&self.span_id, SpanStatus::Error(error.to_string()));
    }
}

impl Drop for TestSpan {
    fn drop(&mut self) {
        // Auto-complete span if not explicitly completed
        if let Some(span) = self.tracer.active_spans.read().get(&self.span_id) {
            if matches!(span.status, SpanStatus::Active) {
                self.tracer.complete_span(&self.span_id, SpanStatus::Ok);
            }
        }
    }
}

/// Test logger for structured logging
#[derive(Debug, Clone)]
pub struct TestLogger {
    /// Logger name
    name: String,
    /// Log level filter
    level_filter: EnvFilter,
    /// Custom fields
    fields: HashMap<String, String>,
}

impl TestLogger {
    /// Creates a new test logger
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            level_filter: EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
            fields: HashMap::new(),
        }
    }
    
    /// Adds a field to all log entries
    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Sets the log level
    pub fn with_level(mut self, level: &str) -> Self {
        if let Ok(filter) = EnvFilter::try_new(level) {
            self.level_filter = filter;
        }
        self
    }
}

/// System information collector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Operating system
    pub os_name: String,
    /// OS version
    pub os_version: String,
    /// Architecture
    pub architecture: String,
    /// Number of CPU cores
    pub cpu_cores: usize,
    /// CPU model
    pub cpu_model: String,
    /// Total memory in bytes
    pub total_memory_bytes: u64,
    /// Available memory in bytes
    pub available_memory_bytes: u64,
    /// Hostname
    pub hostname: String,
    /// Rust version
    pub rust_version: String,
    /// Environment variables (filtered)
    pub environment: HashMap<String, String>,
}

/// Baseline performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineMetrics {
    /// CPU benchmark score
    pub cpu_score: f64,
    /// Memory bandwidth (GB/s)
    pub memory_bandwidth_gbps: f64,
    /// Disk I/O speed (MB/s)
    pub disk_io_mbps: f64,
    /// Network baseline latency (microseconds)
    pub network_latency_us: f64,
    /// Context switch time (nanoseconds)
    pub context_switch_ns: f64,
    /// Timestamp when baseline was measured
    pub measured_at: SystemTime,
}

/// Sets up comprehensive test observability
pub fn setup_test_observability() -> Result<(TestTracer, TestLogger), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber with multiple layers
    let tracer = TestTracer::new();
    let logger = TestLogger::new("memorystreamer-tests");
    
    // Set up structured logging
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .json(); // Use JSON format for structured logging
    
    let env_filter = logger.level_filter.clone();
    
    tracing_subscriber::registry()
        .with(fmt_layer.with_filter(env_filter))
        .init();
    
    info!("Test observability initialized");
    
    Ok((tracer, logger))
}

/// Collects comprehensive system information
pub fn collect_system_info() -> SystemInfo {
    let mut environment = HashMap::new();
    
    // Collect relevant environment variables
    for (key, value) in std::env::vars() {
        // Filter sensitive information
        if !key.contains("PASSWORD") && !key.contains("SECRET") && !key.contains("TOKEN") {
            environment.insert(key, value);
        }
    }
    
    // In a real implementation, these would use system APIs
    SystemInfo {
        os_name: std::env::consts::OS.to_string(),
        os_version: "unknown".to_string(), // Would use platform-specific APIs
        architecture: std::env::consts::ARCH.to_string(),
        cpu_cores: num_cpus::get(),
        cpu_model: "unknown".to_string(), // Would read from /proc/cpuinfo on Linux
        total_memory_bytes: 0, // Would use system APIs
        available_memory_bytes: 0, // Would use system APIs
        hostname: hostname().unwrap_or_else(|| "unknown".to_string()),
        rust_version: env!("CARGO_PKG_RUST_VERSION").to_string(),
        environment,
    }
}

/// Runs baseline performance benchmarks
pub fn benchmark_baseline() -> BaselineMetrics {
    info!("Running baseline performance benchmarks");
    
    let start_time = std::time::Instant::now();
    
    // CPU benchmark (simple arithmetic)
    let cpu_score = benchmark_cpu();
    
    // Memory benchmark
    let memory_bandwidth = benchmark_memory();
    
    // Context switch benchmark
    let context_switch_time = benchmark_context_switch();
    
    let benchmark_duration = start_time.elapsed();
    info!("Baseline benchmarks completed in {:?}", benchmark_duration);
    
    BaselineMetrics {
        cpu_score,
        memory_bandwidth_gbps: memory_bandwidth,
        disk_io_mbps: 0.0, // Would implement disk I/O benchmark
        network_latency_us: 0.0, // Would implement network benchmark
        context_switch_ns: context_switch_time,
        measured_at: SystemTime::now(),
    }
}

/// Benchmarks CPU performance
fn benchmark_cpu() -> f64 {
    let iterations = 1_000_000;
    let start = std::time::Instant::now();
    
    let mut sum = 0u64;
    for i in 0..iterations {
        sum = sum.wrapping_add(i * i);
    }
    
    let duration = start.elapsed();
    
    // Return operations per second
    let ops_per_sec = iterations as f64 / duration.as_secs_f64();
    
    // Prevent optimization from removing the computation
    std::hint::black_box(sum);
    
    ops_per_sec
}

/// Benchmarks memory bandwidth
fn benchmark_memory() -> f64 {
    let size = 1024 * 1024; // 1MB
    let iterations = 1000;
    
    let mut data = vec![0u8; size];
    
    let start = std::time::Instant::now();
    
    for _ in 0..iterations {
        for i in 0..size {
            data[i] = (i % 256) as u8;
        }
        std::hint::black_box(&data);
    }
    
    let duration = start.elapsed();
    let bytes_processed = size * iterations;
    let bandwidth_bps = bytes_processed as f64 / duration.as_secs_f64();
    
    // Convert to GB/s
    bandwidth_bps / (1024.0 * 1024.0 * 1024.0)
}

/// Benchmarks context switch time
fn benchmark_context_switch() -> f64 {
    use std::sync::mpsc;
    use std::thread;
    
    let (tx1, rx1) = mpsc::channel();
    let (tx2, rx2) = mpsc::channel();
    
    let iterations = 10000;
    
    let handle = thread::spawn(move || {
        for _ in 0..iterations {
            rx1.recv().unwrap();
            tx2.send(()).unwrap();
        }
    });
    
    let start = std::time::Instant::now();
    
    for _ in 0..iterations {
        tx1.send(()).unwrap();
        rx2.recv().unwrap();
    }
    
    let duration = start.elapsed();
    handle.join().unwrap();
    
    // Return nanoseconds per context switch (round trip)
    duration.as_nanos() as f64 / (iterations * 2) as f64
}

/// Gets the system hostname
fn hostname() -> Option<String> {
    // Simple hostname detection
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
}

/// Creates a test span with automatic instrumentation
#[macro_export]
macro_rules! test_span {
    ($tracer:expr, $name:expr) => {
        $tracer.start_span($name)
    };
    ($tracer:expr, $name:expr, $($key:expr => $value:expr),+) => {
        {
            let span = $tracer.start_span($name);
            $(
                span.set_attribute($key, $value);
            )+
            span
        }
    };
}

/// Creates a test event
#[macro_export]
macro_rules! test_event {
    ($tracer:expr, $name:expr, $message:expr) => {
        {
            let event = $crate::observability::Event {
                name: $name.to_string(),
                timestamp: std::time::SystemTime::now(),
                level: $crate::observability::EventLevel::Info,
                attributes: std::collections::HashMap::new(),
                message: $message.to_string(),
                span_id: None,
            };
            $tracer.record_event(&event);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tracer_creation() {
        let tracer = TestTracer::new();
        let trace_id = tracer.start_trace("test_suite", Some("test_case"));
        
        assert!(!trace_id.is_empty());
        assert!(trace_id.starts_with("trace_"));
        
        let traces = tracer.export_traces();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].trace_id, trace_id);
    }
    
    #[test]
    fn test_span_creation() {
        let tracer = TestTracer::new();
        tracer.start_trace("test_suite", None);
        
        let span = tracer.start_span("test_operation");
        assert!(!span.span_id.is_empty());
        assert_eq!(span.name, "test_operation");
        
        span.set_attribute("test_key", "test_value");
        span.success();
    }
    
    #[test]
    fn test_logger_creation() {
        let logger = TestLogger::new("test_logger")
            .with_field("service", "memorystreamer")
            .with_level("debug");
        
        assert_eq!(logger.name, "test_logger");
        assert_eq!(logger.fields.get("service"), Some(&"memorystreamer".to_string()));
    }
    
    #[test]
    fn test_system_info_collection() {
        let info = collect_system_info();
        
        assert!(!info.os_name.is_empty());
        assert!(!info.architecture.is_empty());
        assert!(info.cpu_cores > 0);
    }
    
    #[test]
    fn test_baseline_benchmarks() {
        let baseline = benchmark_baseline();
        
        assert!(baseline.cpu_score > 0.0);
        assert!(baseline.memory_bandwidth_gbps >= 0.0);
        assert!(baseline.context_switch_ns > 0.0);
    }
    
    #[test]
    fn test_event_creation() {
        let event = Event {
            name: "test_event".to_string(),
            timestamp: SystemTime::now(),
            level: EventLevel::Info,
            attributes: HashMap::new(),
            message: "Test message".to_string(),
            span_id: Some("test_span".to_string()),
        };
        
        assert_eq!(event.name, "test_event");
        assert_eq!(event.message, "Test message");
        assert!(matches!(event.level, EventLevel::Info));
    }
}