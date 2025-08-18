//! # Distributed Tracing System
//!
//! OpenTelemetry-compatible distributed tracing with minimal overhead
//! designed for <5% performance impact when enabled at 1% sampling.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use dashmap::DashMap;
use smallvec::SmallVec;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::telemetry::{TelemetryError, Result, TracingConfig};

/// Distributed tracing system with minimal overhead
pub struct TracingSystem {
    /// Active trace contexts
    active_traces: DashMap<TraceId, Arc<TraceContext>>,
    
    /// Span processor for async span handling
    span_processor: Arc<SpanProcessor>,
    
    /// Trace sampler for controlling overhead
    sampler: Arc<dyn TraceSampler>,
    
    /// Context propagator for distributed tracing
    context_propagator: Arc<ContextPropagator>,
    
    /// Configuration
    config: TracingConfig,
    
    /// Performance metrics
    metrics: TracingMetrics,
    
    /// Span buffer for batched export
    span_buffer: Arc<SpanBuffer>,
    
    /// Background task handle
    processor_handle: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl TracingSystem {
    /// Creates a new tracing system
    pub async fn new(config: TracingConfig) -> Result<Self> {
        let span_buffer = Arc::new(SpanBuffer::new(config.buffer.max_spans));
        let span_processor = Arc::new(SpanProcessor::new(
            config.clone(),
            Arc::clone(&span_buffer),
        ));
        
        let sampler: Arc<dyn TraceSampler> = match config.sampling_rate {
            rate if rate >= 1.0 => Arc::new(AlwaysSampler),
            rate if rate <= 0.0 => Arc::new(NeverSampler),
            rate => Arc::new(ProbabilitySampler::new(rate)),
        };
        
        Ok(Self {
            active_traces: DashMap::new(),
            span_processor,
            sampler,
            context_propagator: Arc::new(ContextPropagator::new()),
            config,
            metrics: TracingMetrics::new(),
            span_buffer,
            processor_handle: parking_lot::Mutex::new(None),
        })
    }

    /// Starts a new span with optional parent context
    ///
    /// # Performance
    /// Target: <1μs for span creation when not sampled
    /// Target: <10μs for span creation when sampled
    #[inline]
    pub fn start_span(&self, name: &str, parent: Option<TraceContext>) -> Span {
        let (trace_id, parent_span_id) = match parent {
            Some(ref ctx) => (ctx.trace_id, Some(ctx.span_id)),
            None => (TraceId::generate(), None),
        };

        let span_id = SpanId::generate();
        let start_time = Instant::now();
        
        // Check sampling decision
        let sampling_decision = self.sampler.should_sample(&SamplingInput {
            trace_id,
            span_id,
            parent_span_id,
            name,
            kind: SpanKind::Internal,
            attributes: &[],
        });

        let context = TraceContext {
            trace_id,
            span_id,
            parent_span_id,
            sampling_decision,
            baggage: HashMap::new(),
        };

        // Create span
        let span = Span {
            context: context.clone(),
            name: name.to_string(),
            start_time,
            end_time: None,
            status: SpanStatus::Unset,
            kind: SpanKind::Internal,
            attributes: SmallVec::new(),
            events: SmallVec::new(),
            links: SmallVec::new(),
        };

        // Store active trace context if sampled
        if sampling_decision.is_sampled() {
            let trace_context = Arc::new(context);
            self.active_traces.insert(trace_id, trace_context);
        }

        // Update metrics
        self.metrics.spans_started.fetch_add(1, Ordering::Relaxed);
        if sampling_decision.is_sampled() {
            self.metrics.spans_sampled.fetch_add(1, Ordering::Relaxed);
        }

        span
    }

    /// Ends a span and submits it for processing
    #[inline]
    pub fn end_span(&self, mut span: Span) {
        span.end_time = Some(Instant::now());
        
        // Only process sampled spans
        if span.context.sampling_decision.is_sampled() {
            // Remove from active traces
            self.active_traces.remove(&span.context.trace_id);
            
            // Submit to span processor
            if let Err(e) = self.span_processor.process_span(span) {
                self.metrics.processing_errors.fetch_add(1, Ordering::Relaxed);
                tracing::warn!("Failed to process span: {}", e);
            }
        }

        self.metrics.spans_ended.fetch_add(1, Ordering::Relaxed);
    }

    /// Propagates trace context to outgoing headers
    pub fn propagate_context(&self, context: &TraceContext, headers: &mut HashMap<String, String>) {
        self.context_propagator.inject(context, headers);
    }

    /// Extracts trace context from incoming headers
    pub fn extract_context(&self, headers: &HashMap<String, String>) -> Option<TraceContext> {
        self.context_propagator.extract(headers)
    }

    /// Returns the number of active traces
    pub fn active_traces_count(&self) -> usize {
        self.active_traces.len()
    }

    /// Starts the background span processor
    pub async fn start_span_processor(&self) -> Result<()> {
        let processor = Arc::clone(&self.span_processor);
        let config = self.config.clone();
        
        let handle = tokio::spawn(async move {
            processor.run_background_processing(config).await;
        });

        *self.processor_handle.lock() = Some(handle);
        Ok(())
    }

    /// Stops the background span processor
    pub async fn stop_span_processor(&self) -> Result<()> {
        if let Some(handle) = self.processor_handle.lock().take() {
            handle.abort();
            let _ = handle.await;
        }
        Ok(())
    }

    /// Flushes all pending spans
    pub async fn flush_spans(&self) -> Result<()> {
        self.span_processor.flush().await
    }

    /// Returns current tracing metrics
    pub fn metrics(&self) -> TracingMetricsSnapshot {
        TracingMetricsSnapshot {
            spans_started: self.metrics.spans_started.load(Ordering::Relaxed),
            spans_ended: self.metrics.spans_ended.load(Ordering::Relaxed),
            spans_sampled: self.metrics.spans_sampled.load(Ordering::Relaxed),
            spans_dropped: self.metrics.spans_dropped.load(Ordering::Relaxed),
            processing_errors: self.metrics.processing_errors.load(Ordering::Relaxed),
            active_traces: self.active_traces.len(),
            buffer_size: self.span_buffer.size(),
            buffer_capacity: self.span_buffer.capacity(),
        }
    }

    /// Returns estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let traces_size = self.active_traces.len() * std::mem::size_of::<(TraceId, Arc<TraceContext>)>();
        let buffer_size = self.span_buffer.memory_usage();
        
        base_size + traces_size + buffer_size
    }

    /// Returns estimated CPU usage
    pub fn cpu_usage_estimate(&self) -> f64 {
        // Estimate based on span processing rate and sampling
        let total_spans = self.metrics.spans_started.load(Ordering::Relaxed);
        let sampling_rate = self.config.sampling_rate;
        
        // Very rough estimate: each sampled span costs ~1μs of CPU
        let spans_per_second = total_spans as f64 / 60.0; // Assume 1-minute average
        spans_per_second * sampling_rate * 0.000001 // 1μs per span
    }
}

/// Trace identifier (128-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId([u8; 16]);

impl TraceId {
    /// Generates a new random trace ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4().into_bytes())
    }

    /// Creates a trace ID from bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the trace ID as bytes
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns the trace ID as a hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parses a trace ID from a hex string
    pub fn from_hex(hex: &str) -> Result<Self> {
        let bytes = hex::decode(hex)
            .map_err(|e| TelemetryError::tracing(format!("Invalid trace ID hex: {}", e)))?;
        
        if bytes.len() != 16 {
            return Err(TelemetryError::tracing("Trace ID must be 16 bytes"));
        }

        let mut array = [0u8; 16];
        array.copy_from_slice(&bytes);
        Ok(Self(array))
    }
}

/// Span identifier (64-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId([u8; 8]);

impl SpanId {
    /// Generates a new random span ID
    pub fn generate() -> Self {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&Uuid::new_v4().as_bytes()[..8]);
        Self(bytes)
    }

    /// Creates a span ID from bytes
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Returns the span ID as bytes
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// Returns the span ID as a hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parses a span ID from a hex string
    pub fn from_hex(hex: &str) -> Result<Self> {
        let bytes = hex::decode(hex)
            .map_err(|e| TelemetryError::tracing(format!("Invalid span ID hex: {}", e)))?;
        
        if bytes.len() != 8 {
            return Err(TelemetryError::tracing("Span ID must be 8 bytes"));
        }

        let mut array = [0u8; 8];
        array.copy_from_slice(&bytes);
        Ok(Self(array))
    }
}

/// Trace context for propagation
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub sampling_decision: SamplingDecision,
    pub baggage: HashMap<String, String>,
}

/// Sampling decision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingDecision {
    /// Drop the span
    Drop,
    /// Record the span
    Record,
    /// Record and sample the span
    RecordAndSample,
}

impl SamplingDecision {
    pub fn is_sampled(self) -> bool {
        matches!(self, Self::RecordAndSample)
    }

    pub fn is_recorded(self) -> bool {
        matches!(self, Self::Record | Self::RecordAndSample)
    }
}

/// Span representation
#[derive(Debug, Clone)]
pub struct Span {
    pub context: TraceContext,
    pub name: String,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub status: SpanStatus,
    pub kind: SpanKind,
    pub attributes: SmallVec<[Attribute; 8]>,
    pub events: SmallVec<[Event; 4]>,
    pub links: SmallVec<[Link; 2]>,
}

impl Span {
    /// Adds an attribute to the span
    pub fn set_attribute(&mut self, key: &str, value: AttributeValue) {
        self.attributes.push(Attribute {
            key: key.to_string(),
            value,
        });
    }

    /// Adds an event to the span
    pub fn add_event(&mut self, name: &str, attributes: Vec<Attribute>) {
        self.events.push(Event {
            name: name.to_string(),
            timestamp: Instant::now(),
            attributes,
        });
    }

    /// Sets the span status
    pub fn set_status(&mut self, status: SpanStatus) {
        self.status = status;
    }

    /// Sets the span kind
    pub fn set_kind(&mut self, kind: SpanKind) {
        self.kind = kind;
    }

    /// Returns the span duration if ended
    pub fn duration(&self) -> Option<Duration> {
        self.end_time.map(|end| end.duration_since(self.start_time))
    }
}

/// Span status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanStatus {
    Unset,
    Ok,
    Error { message: String },
}

/// Span kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
}

/// Span attribute
#[derive(Debug, Clone)]
pub struct Attribute {
    pub key: String,
    pub value: AttributeValue,
}

/// Attribute value types
#[derive(Debug, Clone)]
pub enum AttributeValue {
    String(String),
    Bool(bool),
    Int(i64),
    Double(f64),
    Array(Vec<AttributeValue>),
}

/// Span event
#[derive(Debug, Clone)]
pub struct Event {
    pub name: String,
    pub timestamp: Instant,
    pub attributes: Vec<Attribute>,
}

/// Span link
#[derive(Debug, Clone)]
pub struct Link {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub attributes: Vec<Attribute>,
}

/// Trace sampler trait
pub trait TraceSampler: Send + Sync {
    fn should_sample(&self, input: &SamplingInput) -> SamplingDecision;
}

/// Sampling input data
pub struct SamplingInput<'a> {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub name: &'a str,
    pub kind: SpanKind,
    pub attributes: &'a [Attribute],
}

/// Always sample (for debugging)
pub struct AlwaysSampler;

impl TraceSampler for AlwaysSampler {
    fn should_sample(&self, _input: &SamplingInput) -> SamplingDecision {
        SamplingDecision::RecordAndSample
    }
}

/// Never sample (disabled tracing)
pub struct NeverSampler;

impl TraceSampler for NeverSampler {
    fn should_sample(&self, _input: &SamplingInput) -> SamplingDecision {
        SamplingDecision::Drop
    }
}

/// Probability-based sampler
pub struct ProbabilitySampler {
    threshold: u64,
}

impl ProbabilitySampler {
    pub fn new(probability: f64) -> Self {
        let threshold = (probability * (u64::MAX as f64)) as u64;
        Self { threshold }
    }
}

impl TraceSampler for ProbabilitySampler {
    fn should_sample(&self, input: &SamplingInput) -> SamplingDecision {
        // Use trace ID for consistent sampling
        let trace_bytes = input.trace_id.as_bytes();
        let hash = u64::from_be_bytes([
            trace_bytes[0], trace_bytes[1], trace_bytes[2], trace_bytes[3],
            trace_bytes[4], trace_bytes[5], trace_bytes[6], trace_bytes[7],
        ]);
        
        if hash <= self.threshold {
            SamplingDecision::RecordAndSample
        } else {
            SamplingDecision::Drop
        }
    }
}

/// Context propagator for distributed tracing
pub struct ContextPropagator;

impl ContextPropagator {
    pub fn new() -> Self {
        Self
    }

    /// Injects trace context into headers (W3C Trace Context format)
    pub fn inject(&self, context: &TraceContext, headers: &mut HashMap<String, String>) {
        // W3C Trace Context: traceparent header
        let traceparent = format!(
            "00-{}-{}-{:02x}",
            context.trace_id.to_hex(),
            context.span_id.to_hex(),
            if context.sampling_decision.is_sampled() { 1 } else { 0 }
        );
        headers.insert("traceparent".to_string(), traceparent);

        // W3C Trace Context: tracestate header (baggage)
        if !context.baggage.is_empty() {
            let tracestate = context.baggage
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(",");
            headers.insert("tracestate".to_string(), tracestate);
        }
    }

    /// Extracts trace context from headers
    pub fn extract(&self, headers: &HashMap<String, String>) -> Option<TraceContext> {
        let traceparent = headers.get("traceparent")?;
        self.parse_traceparent(traceparent, headers)
    }

    fn parse_traceparent(&self, traceparent: &str, headers: &HashMap<String, String>) -> Option<TraceContext> {
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        // Parse version (should be "00")
        if parts[0] != "00" {
            return None;
        }

        // Parse trace ID
        let trace_id = TraceId::from_hex(parts[1]).ok()?;

        // Parse span ID
        let span_id = SpanId::from_hex(parts[2]).ok()?;

        // Parse flags
        let flags = u8::from_str_radix(parts[3], 16).ok()?;
        let sampling_decision = if flags & 0x01 != 0 {
            SamplingDecision::RecordAndSample
        } else {
            SamplingDecision::Drop
        };

        // Parse baggage from tracestate
        let mut baggage = HashMap::new();
        if let Some(tracestate) = headers.get("tracestate") {
            for pair in tracestate.split(',') {
                if let Some((key, value)) = pair.split_once('=') {
                    baggage.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }

        Some(TraceContext {
            trace_id,
            span_id,
            parent_span_id: None, // Will be set by caller if needed
            sampling_decision,
            baggage,
        })
    }
}

/// Span processor for batched export
pub struct SpanProcessor {
    sender: mpsc::UnboundedSender<SpanProcessorMessage>,
}

impl SpanProcessor {
    pub fn new(config: TracingConfig, buffer: Arc<SpanBuffer>) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        
        Self { sender }
    }

    pub fn process_span(&self, span: Span) -> Result<()> {
        self.sender.send(SpanProcessorMessage::ProcessSpan(span))
            .map_err(|_| TelemetryError::tracing("Span processor channel closed"))?;
        Ok(())
    }

    pub async fn flush(&self) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender.send(SpanProcessorMessage::Flush(sender))
            .map_err(|_| TelemetryError::tracing("Span processor channel closed"))?;
        
        receiver.await
            .map_err(|_| TelemetryError::tracing("Flush operation failed"))?
    }

    pub async fn run_background_processing(&self, config: TracingConfig) {
        // Background processing would be implemented here
        // For brevity, this is a placeholder
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

enum SpanProcessorMessage {
    ProcessSpan(Span),
    Flush(oneshot::Sender<Result<()>>),
    Shutdown,
}

/// Span buffer for batched processing
pub struct SpanBuffer {
    spans: parking_lot::Mutex<Vec<Span>>,
    capacity: usize,
    size: AtomicUsize,
}

impl SpanBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            spans: parking_lot::Mutex::new(Vec::with_capacity(capacity)),
            capacity,
            size: AtomicUsize::new(0),
        }
    }

    pub fn add_span(&self, span: Span) -> bool {
        let mut spans = self.spans.lock();
        if spans.len() < self.capacity {
            spans.push(span);
            self.size.store(spans.len(), Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn drain_spans(&self) -> Vec<Span> {
        let mut spans = self.spans.lock();
        let drained = std::mem::take(&mut *spans);
        self.size.store(0, Ordering::Relaxed);
        drained
    }

    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let spans_size = self.size() * std::mem::size_of::<Span>();
        base_size + spans_size
    }
}

/// Tracing performance metrics
#[derive(Debug)]
pub struct TracingMetrics {
    pub spans_started: AtomicU64,
    pub spans_ended: AtomicU64,
    pub spans_sampled: AtomicU64,
    pub spans_dropped: AtomicU64,
    pub processing_errors: AtomicU64,
}

impl TracingMetrics {
    pub fn new() -> Self {
        Self {
            spans_started: AtomicU64::new(0),
            spans_ended: AtomicU64::new(0),
            spans_sampled: AtomicU64::new(0),
            spans_dropped: AtomicU64::new(0),
            processing_errors: AtomicU64::new(0),
        }
    }
}

/// Snapshot of tracing metrics
#[derive(Debug, Clone)]
pub struct TracingMetricsSnapshot {
    pub spans_started: u64,
    pub spans_ended: u64,
    pub spans_sampled: u64,
    pub spans_dropped: u64,
    pub processing_errors: u64,
    pub active_traces: usize,
    pub buffer_size: usize,
    pub buffer_capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trace_id_generation() {
        let trace_id1 = TraceId::generate();
        let trace_id2 = TraceId::generate();
        
        assert_ne!(trace_id1, trace_id2);
        assert_eq!(trace_id1.as_bytes().len(), 16);
    }

    #[test]
    fn test_trace_id_hex_conversion() {
        let trace_id = TraceId::generate();
        let hex = trace_id.to_hex();
        let parsed = TraceId::from_hex(&hex).unwrap();
        
        assert_eq!(trace_id, parsed);
        assert_eq!(hex.len(), 32);
    }

    #[tokio::test]
    async fn test_span_creation() {
        let config = TracingConfig::default();
        let tracing = TracingSystem::new(config).await.unwrap();
        
        let span = tracing.start_span("test_operation", None);
        assert_eq!(span.name, "test_operation");
        assert!(span.end_time.is_none());
        
        tracing.end_span(span);
        
        let metrics = tracing.metrics();
        assert_eq!(metrics.spans_started, 1);
        assert_eq!(metrics.spans_ended, 1);
    }

    #[test]
    fn test_probability_sampler() {
        let sampler = ProbabilitySampler::new(0.1); // 10% sampling
        
        let mut sampled_count = 0;
        let total_samples = 1000;
        
        for i in 0..total_samples {
            let trace_id = TraceId::generate();
            let span_id = SpanId::generate();
            
            let input = SamplingInput {
                trace_id,
                span_id,
                parent_span_id: None,
                name: "test",
                kind: SpanKind::Internal,
                attributes: &[],
            };
            
            if sampler.should_sample(&input).is_sampled() {
                sampled_count += 1;
            }
        }
        
        // Should be approximately 10% (with some variance)
        let sampling_rate = sampled_count as f64 / total_samples as f64;
        assert!(sampling_rate > 0.05 && sampling_rate < 0.15);
    }

    #[test]
    fn test_context_propagation() {
        let propagator = ContextPropagator::new();
        
        let context = TraceContext {
            trace_id: TraceId::generate(),
            span_id: SpanId::generate(),
            parent_span_id: None,
            sampling_decision: SamplingDecision::RecordAndSample,
            baggage: {
                let mut map = HashMap::new();
                map.insert("user".to_string(), "123".to_string());
                map
            },
        };
        
        let mut headers = HashMap::new();
        propagator.inject(&context, &mut headers);
        
        assert!(headers.contains_key("traceparent"));
        assert!(headers.contains_key("tracestate"));
        
        let extracted = propagator.extract(&headers).unwrap();
        assert_eq!(extracted.trace_id, context.trace_id);
        assert_eq!(extracted.span_id, context.span_id);
        assert_eq!(extracted.sampling_decision, context.sampling_decision);
        assert_eq!(extracted.baggage.get("user"), Some(&"123".to_string()));
    }

    #[bench]
    fn bench_span_creation_not_sampled(b: &mut test::Bencher) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tracing = rt.block_on(async {
            let mut config = TracingConfig::default();
            config.sampling_rate = 0.0; // No sampling
            TracingSystem::new(config).await.unwrap()
        });
        
        b.iter(|| {
            let start = std::time::Instant::now();
            let span = tracing.start_span("test", None);
            tracing.end_span(span);
            let elapsed = start.elapsed();
            
            // Verify performance target (<1μs when not sampled)
            assert!(elapsed < Duration::from_micros(1));
        });
    }

    #[bench]
    fn bench_span_creation_sampled(b: &mut test::Bencher) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tracing = rt.block_on(async {
            let mut config = TracingConfig::default();
            config.sampling_rate = 1.0; // Always sample
            TracingSystem::new(config).await.unwrap()
        });
        
        b.iter(|| {
            let start = std::time::Instant::now();
            let span = tracing.start_span("test", None);
            tracing.end_span(span);
            let elapsed = start.elapsed();
            
            // Verify performance target (<10μs when sampled)
            assert!(elapsed < Duration::from_micros(10));
        });
    }

    #[test]
    fn test_span_buffer() {
        let buffer = SpanBuffer::new(10);
        assert_eq!(buffer.size(), 0);
        assert_eq!(buffer.capacity(), 10);
        
        // Add spans
        for i in 0..5 {
            let span = Span {
                context: TraceContext {
                    trace_id: TraceId::generate(),
                    span_id: SpanId::generate(),
                    parent_span_id: None,
                    sampling_decision: SamplingDecision::RecordAndSample,
                    baggage: HashMap::new(),
                },
                name: format!("span_{}", i),
                start_time: Instant::now(),
                end_time: None,
                status: SpanStatus::Unset,
                kind: SpanKind::Internal,
                attributes: SmallVec::new(),
                events: SmallVec::new(),
                links: SmallVec::new(),
            };
            
            assert!(buffer.add_span(span));
        }
        
        assert_eq!(buffer.size(), 5);
        
        let spans = buffer.drain_spans();
        assert_eq!(spans.len(), 5);
        assert_eq!(buffer.size(), 0);
    }
}