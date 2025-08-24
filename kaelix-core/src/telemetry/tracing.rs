//! # Distributed Tracing System
//!
//! High-performance distributed tracing with OpenTelemetry compatibility.

use crate::telemetry::config::SamplingStrategy;
use crate::telemetry::{Result, TelemetryError, TracingConfig};
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

/// Unique identifier for traces
pub type TraceId = u128;

/// Unique identifier for spans
pub type SpanId = u64;

/// High-performance distributed tracing system
pub struct TracingSystem {
    /// System configuration
    config: TracingConfig,

    /// Active trace contexts
    active_traces: DashMap<TraceId, Arc<TraceContext>>,

    /// Span buffer for batch export
    span_buffer: Arc<Mutex<SpanBuffer>>,

    /// System metrics
    metrics: TracingMetrics,

    /// Sampling state
    sampler: Arc<dyn Sampler + Send + Sync>,

    /// Export state
    export_enabled: AtomicBool,
}

/// Trace context containing span hierarchy
#[derive(Debug)]
pub struct TraceContext {
    /// Trace identifier
    pub trace_id: TraceId,

    /// Root span of the trace
    pub root_span: Arc<Span>,

    /// All spans in this trace
    pub spans: RwLock<HashMap<SpanId, Arc<Span>>>,

    /// Trace creation timestamp
    pub created_at: Instant,

    /// Trace completion status
    pub completed: AtomicBool,
}

/// Individual span within a trace
#[derive(Debug, Serialize, Deserialize)]
pub struct Span {
    /// Span identifier
    pub span_id: SpanId,

    /// Parent span identifier (None for root spans)
    pub parent_span_id: Option<SpanId>,

    /// Trace this span belongs to
    pub trace_id: TraceId,

    /// Operation name
    pub operation_name: String,

    /// Span start time
    pub start_time: SystemTime,

    /// Span end time (None if active)
    pub end_time: Option<SystemTime>,

    /// Span tags/attributes
    pub tags: HashMap<String, String>,

    /// Span logs/events
    pub logs: Vec<SpanLog>,

    /// Span status
    pub status: SpanStatus,
}

/// Span log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLog {
    /// Log timestamp
    pub timestamp: SystemTime,

    /// Log message
    pub message: String,

    /// Log level
    pub level: String,

    /// Additional fields
    pub fields: HashMap<String, String>,
}

/// Span completion status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpanStatus {
    /// Span is still active
    Active,

    /// Span completed successfully
    Ok,

    /// Span completed with error
    Error { message: String },

    /// Span was cancelled
    Cancelled,

    /// Span timed out
    Timeout,
}

/// Sampling decision for traces
pub trait Sampler {
    /// Decide whether to sample a trace
    fn sample(&self, trace_id: TraceId, operation_name: &str) -> bool;

    /// Get current sampling rate (0.0 to 1.0)
    fn sampling_rate(&self) -> f64;
}

/// Always-on sampler (samples all traces)
pub struct AlwaysOnSampler;

impl Sampler for AlwaysOnSampler {
    fn sample(&self, _trace_id: TraceId, _operation_name: &str) -> bool {
        true
    }

    fn sampling_rate(&self) -> f64 {
        1.0
    }
}

/// Always-off sampler (samples no traces)
pub struct AlwaysOffSampler;

impl Sampler for AlwaysOffSampler {
    fn sample(&self, _trace_id: TraceId, _operation_name: &str) -> bool {
        false
    }

    fn sampling_rate(&self) -> f64 {
        0.0
    }
}

/// Percentage-based sampler
pub struct PercentageSampler {
    rate: f64,
}

impl PercentageSampler {
    pub fn new(rate: f64) -> Self {
        Self { rate: rate.clamp(0.0, 1.0) }
    }
}

impl Sampler for PercentageSampler {
    fn sample(&self, trace_id: TraceId, _operation_name: &str) -> bool {
        // Use trace ID for deterministic sampling
        let hash = (trace_id as f64) / (u128::MAX as f64);
        hash < self.rate
    }

    fn sampling_rate(&self) -> f64 {
        self.rate
    }
}

/// Buffered spans for batch export
struct SpanBuffer {
    /// Buffered spans
    spans: Vec<Arc<Span>>,

    /// Buffer capacity
    capacity: usize,

    /// Total bytes used
    bytes_used: usize,
}

impl SpanBuffer {
    fn new(capacity: usize) -> Self {
        Self { spans: Vec::with_capacity(capacity), capacity, bytes_used: 0 }
    }

    fn push(&mut self, span: Arc<Span>) -> bool {
        if self.spans.len() < self.capacity {
            // Rough estimate of span size
            self.bytes_used +=
                std::mem::size_of::<Span>() + span.operation_name.len() + span.tags.len() * 64; // Rough estimate

            self.spans.push(span);
            true
        } else {
            false
        }
    }

    fn drain(&mut self) -> Vec<Arc<Span>> {
        let spans = std::mem::take(&mut self.spans);
        self.bytes_used = 0;
        spans
    }

    fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    fn len(&self) -> usize {
        self.spans.len()
    }

    fn memory_usage(&self) -> usize {
        self.bytes_used
    }
}

/// Tracing system metrics
#[derive(Debug, Default)]
pub struct TracingMetrics {
    /// Number of traces started
    pub traces_started: AtomicU64,

    /// Number of traces completed
    pub traces_completed: AtomicU64,

    /// Number of spans started
    pub spans_started: AtomicU64,

    /// Number of spans completed
    pub spans_completed: AtomicU64,

    /// Number of spans sampled
    pub spans_sampled: AtomicU64,

    /// Number of spans exported
    pub spans_exported: AtomicU64,

    /// Number of export failures
    pub export_failures: AtomicU64,

    /// Current active traces
    pub active_traces: AtomicUsize,

    /// Current active spans
    pub active_spans: AtomicUsize,
}

impl TracingSystem {
    /// Create a new tracing system
    pub fn new(config: TracingConfig) -> Result<Self> {
        let sampler = Self::create_sampler(&config.sampling_strategy)?;

        Ok(Self {
            config: config.clone(),
            active_traces: DashMap::new(),
            span_buffer: Arc::new(Mutex::new(SpanBuffer::new(config.batch_size))),
            metrics: TracingMetrics::default(),
            sampler,
            export_enabled: AtomicBool::new(false),
        })
    }

    /// Create sampler from configuration
    fn create_sampler(strategy: &SamplingStrategy) -> Result<Arc<dyn Sampler + Send + Sync>> {
        let sampler: Arc<dyn Sampler + Send + Sync> = match strategy {
            SamplingStrategy::AlwaysOn => Arc::new(AlwaysOnSampler),
            SamplingStrategy::AlwaysOff => Arc::new(AlwaysOffSampler),
            SamplingStrategy::Percentage(rate) => Arc::new(PercentageSampler::new(*rate)),
            SamplingStrategy::RateLimit(_) => {
                // TODO: Implement rate-limited sampler
                Arc::new(PercentageSampler::new(0.1))
            },
            SamplingStrategy::Adaptive => {
                // TODO: Implement adaptive sampler
                Arc::new(PercentageSampler::new(0.1))
            },
        };

        Ok(sampler)
    }

    /// Start a new trace
    pub fn start_trace(&self, operation_name: String) -> Result<Arc<TraceContext>> {
        let trace_id = self.generate_trace_id();

        // Check sampling decision
        if !self.sampler.sample(trace_id, &operation_name) {
            return Err(TelemetryError::tracing("Trace not sampled".to_string()));
        }

        let root_span = Arc::new(Span {
            span_id: self.generate_span_id(),
            parent_span_id: None,
            trace_id,
            operation_name,
            start_time: SystemTime::now(),
            end_time: None,
            tags: HashMap::new(),
            logs: Vec::new(),
            status: SpanStatus::Active,
        });

        let trace_context = Arc::new(TraceContext {
            trace_id,
            root_span: root_span.clone(),
            spans: RwLock::new(HashMap::new()),
            created_at: Instant::now(),
            completed: AtomicBool::new(false),
        });

        // Add root span to trace
        trace_context.spans.write().insert(root_span.span_id, root_span);

        // Register trace
        self.active_traces.insert(trace_id, trace_context.clone());

        // Update metrics
        self.metrics.traces_started.fetch_add(1, Ordering::Relaxed);
        self.metrics.spans_started.fetch_add(1, Ordering::Relaxed);
        self.metrics.spans_sampled.fetch_add(1, Ordering::Relaxed);
        self.metrics.active_traces.fetch_add(1, Ordering::Relaxed);
        self.metrics.active_spans.fetch_add(1, Ordering::Relaxed);

        Ok(trace_context)
    }

    /// Finish a trace
    pub fn finish_trace(&self, trace_id: TraceId) -> Result<()> {
        if let Some((_, trace_context)) = self.active_traces.remove(&trace_id) {
            trace_context.completed.store(true, Ordering::Release);

            // Export all spans in the trace
            let spans = trace_context.spans.read();
            for span in spans.values() {
                self.export_span(span.clone())?;
            }

            // Update metrics
            self.metrics.traces_completed.fetch_add(1, Ordering::Relaxed);
            self.metrics.active_traces.fetch_sub(1, Ordering::Relaxed);

            Ok(())
        } else {
            Err(TelemetryError::tracing(format!("Trace not found: {}", trace_id)))
        }
    }

    /// Export a span to the buffer
    fn export_span(&self, span: Arc<Span>) -> Result<()> {
        let mut buffer = self.span_buffer.lock();
        if buffer.push(span) {
            self.metrics.spans_exported.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            self.metrics.export_failures.fetch_add(1, Ordering::Relaxed);
            Err(TelemetryError::tracing("Span buffer full".to_string()))
        }
    }

    /// Generate a unique trace ID
    fn generate_trace_id(&self) -> TraceId {
        Uuid::new_v4().as_u128()
    }

    /// Generate a unique span ID
    fn generate_span_id(&self) -> SpanId {
        Uuid::new_v4().as_u128() as u64
    }

    /// Get current metrics snapshot
    pub fn metrics(&self) -> TracingMetricsSnapshot {
        TracingMetricsSnapshot {
            traces_started: self.metrics.traces_started.load(Ordering::Relaxed),
            traces_completed: self.metrics.traces_completed.load(Ordering::Relaxed),
            spans_started: self.metrics.spans_started.load(Ordering::Relaxed),
            spans_completed: self.metrics.spans_completed.load(Ordering::Relaxed),
            spans_sampled: self.metrics.spans_sampled.load(Ordering::Relaxed),
            spans_exported: self.metrics.spans_exported.load(Ordering::Relaxed),
            export_failures: self.metrics.export_failures.load(Ordering::Relaxed),
            active_traces: self.metrics.active_traces.load(Ordering::Relaxed),
            active_spans: self.metrics.active_spans.load(Ordering::Relaxed),
        }
    }

    /// Get memory usage estimation
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let traces_size =
            self.active_traces.len() * std::mem::size_of::<(TraceId, Arc<TraceContext>)>();
        let buffer_size = self.span_buffer.lock().memory_usage();

        base_size + traces_size + buffer_size
    }

    /// Returns estimated CPU usage
    pub fn cpu_usage_estimate(&self) -> f64 {
        // Estimate based on span processing rate and sampling
        let total_spans = self.metrics.spans_started.load(Ordering::Relaxed);
        let sampling_rate = self.sampler.sampling_rate();

        // Very rough estimate: each sampled span costs ~1μs of CPU
        let spans_per_second = total_spans as f64 / 60.0; // Assume 1-minute average
        spans_per_second * sampling_rate * 0.000001 // 1μs per span
    }

    /// Cleanup old traces that have exceeded timeout
    pub fn cleanup_expired_traces(&self) -> Result<usize> {
        let mut cleaned = 0;
        let timeout = Duration::from_millis(self.config.max_span_duration_ms);
        let now = Instant::now();

        let expired_traces: Vec<TraceId> = self
            .active_traces
            .iter()
            .filter_map(|entry| {
                let trace_context = entry.value();
                if now.duration_since(trace_context.created_at) > timeout {
                    Some(*entry.key())
                } else {
                    None
                }
            })
            .collect();

        for trace_id in expired_traces {
            if let Some((_, trace_context)) = self.active_traces.remove(&trace_id) {
                trace_context.completed.store(true, Ordering::Release);
                cleaned += 1;

                // Mark all spans in trace as timed out
                let mut spans = trace_context.spans.write();
                for span in spans.values_mut() {
                    if let Some(mut_span) = Arc::get_mut(span) {
                        mut_span.status = SpanStatus::Timeout;
                        mut_span.end_time = Some(SystemTime::now());
                    }
                }
            }
        }

        self.metrics.active_traces.fetch_sub(cleaned, Ordering::Relaxed);

        Ok(cleaned)
    }

    /// Flush buffered spans for export
    pub async fn flush(&self) -> Result<Vec<Arc<Span>>> {
        let spans = self.span_buffer.lock().drain();
        Ok(spans)
    }

    /// Shutdown the tracing system
    pub async fn shutdown(&self) -> Result<()> {
        // Finish all active traces
        let trace_ids: Vec<TraceId> = self.active_traces.iter().map(|entry| *entry.key()).collect();
        for trace_id in trace_ids {
            let _ = self.finish_trace(trace_id);
        }

        // Flush remaining spans
        let _ = self.flush().await;

        Ok(())
    }
}

/// Snapshot of tracing metrics
#[derive(Debug, Clone)]
pub struct TracingMetricsSnapshot {
    pub traces_started: u64,
    pub traces_completed: u64,
    pub spans_started: u64,
    pub spans_completed: u64,
    pub spans_sampled: u64,
    pub spans_exported: u64,
    pub export_failures: u64,
    pub active_traces: usize,
    pub active_spans: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracing_system_creation() {
        let config = TracingConfig::default();
        let system = TracingSystem::new(config);
        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_start_and_finish_trace() {
        let config = TracingConfig::default();
        let system = TracingSystem::new(config).unwrap();

        let trace = system.start_trace("test_operation".to_string()).unwrap();
        let trace_id = trace.trace_id;

        let metrics_before = system.metrics();
        assert_eq!(metrics_before.traces_started, 1);
        assert_eq!(metrics_before.active_traces, 1);

        system.finish_trace(trace_id).unwrap();

        let metrics_after = system.metrics();
        assert_eq!(metrics_after.traces_completed, 1);
        assert_eq!(metrics_after.active_traces, 0);
    }

    #[tokio::test]
    async fn test_sampling_always_off() {
        let config =
            TracingConfig { sampling_strategy: SamplingStrategy::AlwaysOff, ..Default::default() };
        let system = TracingSystem::new(config).unwrap();

        let result = system.start_trace("test_operation".to_string());
        assert!(result.is_err()); // Should not sample

        let metrics = system.metrics();
        assert_eq!(metrics.traces_started, 0);
    }

    #[tokio::test]
    async fn test_sampling_percentage() {
        let config = TracingConfig {
            sampling_strategy: SamplingStrategy::Percentage(0.5),
            ..Default::default()
        };
        let system = TracingSystem::new(config).unwrap();

        // Test multiple traces to see sampling behavior
        let mut sampled = 0;
        for i in 0..100 {
            let operation_name = format!("test_operation_{}", i);
            if system.start_trace(operation_name).is_ok() {
                sampled += 1;
            }
        }

        // With 50% sampling and 100 attempts, we expect roughly 50 sampled traces
        // Allow some variance due to probabilistic nature
        assert!(sampled > 30 && sampled < 70);
    }

    #[tokio::test]
    async fn test_memory_usage_tracking() {
        let config = TracingConfig::default();
        let system = TracingSystem::new(config).unwrap();

        let initial_memory = system.memory_usage();

        // Start some traces
        for i in 0..10 {
            let _ = system.start_trace(format!("test_operation_{}", i));
        }

        let after_memory = system.memory_usage();
        assert!(after_memory > initial_memory);
    }

    #[tokio::test]
    async fn test_cleanup_expired_traces() {
        let config = TracingConfig {
            max_span_duration_ms: 1, // Very short timeout for testing
            ..Default::default()
        };
        let system = TracingSystem::new(config).unwrap();

        // Start a trace
        let _trace = system.start_trace("test_operation".to_string()).unwrap();

        // Wait for timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Cleanup should remove the expired trace
        let cleaned = system.cleanup_expired_traces().unwrap();
        assert_eq!(cleaned, 1);

        let metrics = system.metrics();
        assert_eq!(metrics.active_traces, 0);
    }

    #[test]
    fn test_span_buffer() {
        let mut buffer = SpanBuffer::new(2);
        assert!(buffer.is_empty());

        let span1 = Arc::new(Span {
            span_id: 1,
            parent_span_id: None,
            trace_id: 1,
            operation_name: "test1".to_string(),
            start_time: SystemTime::now(),
            end_time: None,
            tags: HashMap::new(),
            logs: Vec::new(),
            status: SpanStatus::Active,
        });

        let span2 = Arc::new(Span {
            span_id: 2,
            parent_span_id: None,
            trace_id: 2,
            operation_name: "test2".to_string(),
            start_time: SystemTime::now(),
            end_time: None,
            tags: HashMap::new(),
            logs: Vec::new(),
            status: SpanStatus::Active,
        });

        assert!(buffer.push(span1));
        assert_eq!(buffer.len(), 1);

        assert!(buffer.push(span2));
        assert_eq!(buffer.len(), 2);

        // Buffer is full
        let span3 = Arc::new(Span {
            span_id: 3,
            parent_span_id: None,
            trace_id: 3,
            operation_name: "test3".to_string(),
            start_time: SystemTime::now(),
            end_time: None,
            tags: HashMap::new(),
            logs: Vec::new(),
            status: SpanStatus::Active,
        });
        assert!(!buffer.push(span3)); // Should fail

        let drained = buffer.drain();
        assert_eq!(drained.len(), 2);
        assert!(buffer.is_empty());
    }
}
