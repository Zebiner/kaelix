//! Plugin execution context and processing environment.

use crate::message::Message;
use crate::types::{Offset, PartitionId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Processing context provided to plugins during message processing.
///
/// Contains metadata and environment information that plugins can use
/// to make processing decisions without requiring direct access to
/// system internals.
///
/// # Performance Considerations
///
/// This context is passed to every plugin invocation, so it must be:
/// - Lightweight to create and clone
/// - Contain only essential information
/// - Use efficient data structures (Arc for shared data)
/// - Avoid unnecessary allocations
#[derive(Debug, Clone)]
pub struct ProcessingContext {
    /// Unique identifier for this processing session
    pub session_id: uuid::Uuid,

    /// Timestamp when processing started
    pub processing_started: Instant,

    /// Message routing information
    pub routing: RoutingContext,

    /// Performance metrics context
    pub metrics: MetricsContext,

    /// Plugin chain information
    pub plugin_chain: PluginChainContext,

    /// Shared metadata that can be passed between plugins
    pub shared_metadata: Arc<parking_lot::RwLock<HashMap<String, String>>>,

    /// Processing configuration and limits
    pub limits: ProcessingLimits,

    /// Tracing and correlation information
    pub tracing: TracingContext,
}

impl ProcessingContext {
    /// Create a new processing context for a message.
    pub fn new(message: &Message) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4(),
            processing_started: Instant::now(),
            routing: RoutingContext::from_message(message),
            metrics: MetricsContext::new(),
            plugin_chain: PluginChainContext::new(),
            shared_metadata: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            limits: ProcessingLimits::default(),
            tracing: TracingContext::new(),
        }
    }

    /// Create a context with custom limits.
    pub fn with_limits(mut self, limits: ProcessingLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Create a context with tracing information.
    pub fn with_tracing(mut self, trace_id: String, span_id: Option<String>) -> Self {
        self.tracing.trace_id = Some(trace_id);
        self.tracing.span_id = span_id;
        self
    }

    /// Get elapsed processing time.
    pub fn elapsed_time(&self) -> Duration {
        self.processing_started.elapsed()
    }

    /// Check if processing has exceeded time limits.
    pub fn is_timeout_exceeded(&self) -> bool {
        if let Some(timeout) = self.limits.max_processing_time {
            self.elapsed_time() > timeout
        } else {
            false
        }
    }

    /// Add metadata that can be shared between plugins.
    pub fn set_shared_metadata(&self, key: String, value: String) {
        self.shared_metadata.write().insert(key, value);
    }

    /// Get shared metadata value.
    pub fn get_shared_metadata(&self, key: &str) -> Option<String> {
        self.shared_metadata.read().get(key).cloned()
    }

    /// Record plugin execution metrics.
    pub fn record_plugin_execution(&mut self, plugin_id: crate::plugin::PluginId, duration: Duration) {
        self.metrics.plugin_executions.push(PluginExecutionMetric {
            plugin_id,
            duration,
            timestamp: Instant::now(),
        });
    }

    /// Get total processing time across all plugins.
    pub fn total_plugin_time(&self) -> Duration {
        self.metrics.plugin_executions.iter().map(|m| m.duration).sum()
    }

    /// Get the number of plugins that have processed this message.
    pub fn plugin_execution_count(&self) -> usize {
        self.metrics.plugin_executions.len()
    }

    /// Check if resource limits have been exceeded.
    pub fn check_resource_limits(&self) -> Result<(), ProcessingLimitError> {
        // Check processing time limit
        if let Some(max_time) = self.limits.max_processing_time {
            if self.elapsed_time() > max_time {
                return Err(ProcessingLimitError::TimeoutExceeded {
                    elapsed: self.elapsed_time(),
                    limit: max_time,
                });
            }
        }

        // Check plugin chain length limit
        if self.plugin_execution_count() > self.limits.max_plugin_chain_length {
            return Err(ProcessingLimitError::ChainLengthExceeded {
                count: self.plugin_execution_count(),
                limit: self.limits.max_plugin_chain_length,
            });
        }

        // Check memory usage (if tracking is enabled)
        if let Some(max_memory) = self.limits.max_memory_usage {
            let current_memory = self.estimate_memory_usage();
            if current_memory > max_memory {
                return Err(ProcessingLimitError::MemoryExceeded {
                    current: current_memory,
                    limit: max_memory,
                });
            }
        }

        Ok(())
    }

    /// Estimate current memory usage of the context.
    fn estimate_memory_usage(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();
        
        // Estimate shared metadata size
        let metadata = self.shared_metadata.read();
        for (key, value) in metadata.iter() {
            size += key.len() + value.len();
        }
        
        // Add plugin execution metrics size
        size += self.metrics.plugin_executions.len() * std::mem::size_of::<PluginExecutionMetric>();
        
        size
    }
}

/// Message routing context information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingContext {
    /// Source topic of the message
    pub source_topic: String,

    /// Target partition (if specified)
    pub target_partition: Option<PartitionId>,

    /// Message offset (if available)
    pub message_offset: Option<Offset>,

    /// Message timestamp
    pub message_timestamp: Timestamp,

    /// Routing metadata
    pub routing_metadata: HashMap<String, String>,
}

impl RoutingContext {
    /// Create routing context from a message.
    pub fn from_message(message: &Message) -> Self {
        Self {
            source_topic: message.topic.as_str().to_string(),
            target_partition: message.partition,
            message_offset: message.offset,
            message_timestamp: message.timestamp,
            routing_metadata: HashMap::new(),
        }
    }

    /// Set routing metadata.
    pub fn set_routing_metadata(&mut self, key: String, value: String) {
        self.routing_metadata.insert(key, value);
    }

    /// Get routing metadata.
    pub fn get_routing_metadata(&self, key: &str) -> Option<&str> {
        self.routing_metadata.get(key).map(String::as_str)
    }
}

/// Performance metrics tracking during processing.
#[derive(Debug, Clone, Default)]
pub struct MetricsContext {
    /// Individual plugin execution metrics
    pub plugin_executions: Vec<PluginExecutionMetric>,

    /// Custom metrics added during processing
    pub custom_metrics: HashMap<String, f64>,

    /// Processing stage timestamps
    pub stage_timestamps: HashMap<String, Instant>,
}

impl MetricsContext {
    /// Create a new metrics context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a custom metric.
    pub fn record_metric(&mut self, name: String, value: f64) {
        self.custom_metrics.insert(name, value);
    }

    /// Record a processing stage timestamp.
    pub fn record_stage(&mut self, stage: String) {
        self.stage_timestamps.insert(stage, Instant::now());
    }

    /// Get time elapsed between two stages.
    pub fn stage_duration(&self, start_stage: &str, end_stage: &str) -> Option<Duration> {
        let start = self.stage_timestamps.get(start_stage)?;
        let end = self.stage_timestamps.get(end_stage)?;
        Some(end.duration_since(*start))
    }

    /// Get all custom metrics.
    pub fn get_custom_metrics(&self) -> &HashMap<String, f64> {
        &self.custom_metrics
    }
}

/// Plugin execution metric.
#[derive(Debug, Clone)]
pub struct PluginExecutionMetric {
    /// Plugin that executed
    pub plugin_id: crate::plugin::PluginId,

    /// Execution duration
    pub duration: Duration,

    /// Execution timestamp
    pub timestamp: Instant,
}

/// Plugin chain execution context.
#[derive(Debug, Clone, Default)]
pub struct PluginChainContext {
    /// Plugins that have processed this message (in order)
    pub executed_plugins: Vec<crate::plugin::PluginId>,

    /// Current position in the plugin chain
    pub current_position: usize,

    /// Total number of plugins in the chain
    pub total_plugins: usize,

    /// Plugin chain metadata
    pub chain_metadata: HashMap<String, String>,
}

impl PluginChainContext {
    /// Create a new plugin chain context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record plugin execution.
    pub fn record_plugin_execution(&mut self, plugin_id: crate::plugin::PluginId) {
        self.executed_plugins.push(plugin_id);
        self.current_position = self.executed_plugins.len();
    }

    /// Check if this is the first plugin in the chain.
    pub fn is_first_plugin(&self) -> bool {
        self.current_position <= 1
    }

    /// Check if this is the last plugin in the chain.
    pub fn is_last_plugin(&self) -> bool {
        self.current_position >= self.total_plugins
    }

    /// Get the percentage of completion through the plugin chain.
    pub fn completion_percentage(&self) -> f64 {
        if self.total_plugins == 0 {
            0.0
        } else {
            (self.current_position as f64 / self.total_plugins as f64) * 100.0
        }
    }

    /// Set chain metadata.
    pub fn set_chain_metadata(&mut self, key: String, value: String) {
        self.chain_metadata.insert(key, value);
    }

    /// Get chain metadata.
    pub fn get_chain_metadata(&self, key: &str) -> Option<&str> {
        self.chain_metadata.get(key).map(String::as_str)
    }
}

/// Processing limits and resource constraints.
#[derive(Debug, Clone)]
pub struct ProcessingLimits {
    /// Maximum total processing time
    pub max_processing_time: Option<Duration>,

    /// Maximum number of plugins in the processing chain
    pub max_plugin_chain_length: usize,

    /// Maximum memory usage during processing
    pub max_memory_usage: Option<usize>,

    /// Maximum size of shared metadata
    pub max_shared_metadata_size: usize,

    /// Maximum number of custom metrics
    pub max_custom_metrics: usize,
}

impl Default for ProcessingLimits {
    fn default() -> Self {
        Self {
            max_processing_time: Some(Duration::from_secs(30)), // 30 second default timeout
            max_plugin_chain_length: 100, // Maximum 100 plugins in a chain
            max_memory_usage: Some(10 * 1024 * 1024), // 10MB default memory limit
            max_shared_metadata_size: 1024, // 1KB metadata limit
            max_custom_metrics: 100, // Maximum 100 custom metrics
        }
    }
}

impl ProcessingLimits {
    /// Create limits for high-performance scenarios.
    pub fn high_performance() -> Self {
        Self {
            max_processing_time: Some(Duration::from_millis(100)), // 100ms timeout
            max_plugin_chain_length: 10, // Limited chain length
            max_memory_usage: Some(1024 * 1024), // 1MB memory limit
            max_shared_metadata_size: 256, // 256 bytes metadata
            max_custom_metrics: 10, // Limited metrics
        }
    }

    /// Create limits for development and testing.
    pub fn development() -> Self {
        Self {
            max_processing_time: Some(Duration::from_secs(300)), // 5 minute timeout
            max_plugin_chain_length: 1000, // Large chain length for testing
            max_memory_usage: Some(100 * 1024 * 1024), // 100MB memory limit
            max_shared_metadata_size: 10 * 1024, // 10KB metadata
            max_custom_metrics: 1000, // Many metrics for debugging
        }
    }

    /// Create unlimited processing limits (use with caution).
    pub fn unlimited() -> Self {
        Self {
            max_processing_time: None,
            max_plugin_chain_length: usize::MAX,
            max_memory_usage: None,
            max_shared_metadata_size: usize::MAX,
            max_custom_metrics: usize::MAX,
        }
    }
}

/// Tracing and correlation context for observability.
#[derive(Debug, Clone, Default)]
pub struct TracingContext {
    /// Distributed trace ID
    pub trace_id: Option<String>,

    /// Span ID within the trace
    pub span_id: Option<String>,

    /// Parent span ID
    pub parent_span_id: Option<String>,

    /// Baggage items (key-value pairs propagated through the trace)
    pub baggage: HashMap<String, String>,

    /// Sampling decision
    pub sampled: bool,
}

impl TracingContext {
    /// Create a new tracing context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a tracing context with IDs.
    pub fn with_ids(trace_id: String, span_id: String, parent_span_id: Option<String>) -> Self {
        Self {
            trace_id: Some(trace_id),
            span_id: Some(span_id),
            parent_span_id,
            baggage: HashMap::new(),
            sampled: true,
        }
    }

    /// Add baggage item.
    pub fn add_baggage(&mut self, key: String, value: String) {
        self.baggage.insert(key, value);
    }

    /// Get baggage item.
    pub fn get_baggage(&self, key: &str) -> Option<&str> {
        self.baggage.get(key).map(String::as_str)
    }

    /// Check if this context has tracing information.
    pub fn has_trace_info(&self) -> bool {
        self.trace_id.is_some() && self.span_id.is_some()
    }

    /// Generate a new child span ID.
    pub fn new_child_span_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// Processing limit violation errors.
#[derive(Debug, thiserror::Error)]
pub enum ProcessingLimitError {
    #[error("Processing timeout exceeded: elapsed {elapsed:?}, limit {limit:?}")]
    TimeoutExceeded {
        elapsed: Duration,
        limit: Duration,
    },

    #[error("Plugin chain length exceeded: count {count}, limit {limit}")]
    ChainLengthExceeded {
        count: usize,
        limit: usize,
    },

    #[error("Memory usage exceeded: current {current} bytes, limit {limit} bytes")]
    MemoryExceeded {
        current: usize,
        limit: usize,
    },

    #[error("Shared metadata size exceeded")]
    MetadataSizeExceeded,

    #[error("Custom metrics limit exceeded")]
    MetricsLimitExceeded,
}

/// Plugin context for plugin-specific state and configuration.
#[derive(Debug, Clone)]
pub struct PluginContext {
    /// Plugin identifier
    pub plugin_id: crate::plugin::PluginId,

    /// Plugin name
    pub plugin_name: String,

    /// Plugin version
    pub plugin_version: semver::Version,

    /// Plugin-specific configuration
    pub configuration: HashMap<String, String>,

    /// Plugin capabilities
    pub capabilities: crate::plugin::PluginCapabilities,

    /// Plugin resource limits
    pub resource_limits: crate::plugin::ResourceLimits,

    /// Plugin creation timestamp
    pub created_at: Instant,
}

impl PluginContext {
    /// Create a new plugin context.
    pub fn new(
        plugin_id: crate::plugin::PluginId,
        plugin_name: String,
        plugin_version: semver::Version,
    ) -> Self {
        Self {
            plugin_id,
            plugin_name,
            plugin_version,
            configuration: HashMap::new(),
            capabilities: crate::plugin::PluginCapabilities::default(),
            resource_limits: crate::plugin::ResourceLimits::default(),
            created_at: Instant::now(),
        }
    }

    /// Set plugin configuration.
    pub fn with_configuration(mut self, config: HashMap<String, String>) -> Self {
        self.configuration = config;
        self
    }

    /// Set plugin capabilities.
    pub fn with_capabilities(mut self, capabilities: crate::plugin::PluginCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Set plugin resource limits.
    pub fn with_resource_limits(mut self, limits: crate::plugin::ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }

    /// Get plugin uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get configuration value.
    pub fn get_config(&self, key: &str) -> Option<&str> {
        self.configuration.get(key).map(String::as_str)
    }

    /// Set configuration value.
    pub fn set_config(&mut self, key: String, value: String) {
        self.configuration.insert(key, value);
    }

    /// Check if plugin has a specific capability.
    pub fn has_capability(&self, _capability: &str) -> bool {
        // This would need to be implemented based on the PluginCapabilities structure
        // For now, return false as a placeholder
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_processing_context_creation() {
        let message = Message::new("test.topic", Bytes::from("test")).unwrap();
        let context = ProcessingContext::new(&message);

        assert_eq!(context.routing.source_topic, "test.topic");
        assert!(!context.is_timeout_exceeded());
        assert_eq!(context.plugin_execution_count(), 0);
    }

    #[test]
    fn test_processing_context_with_limits() {
        let message = Message::new("test.topic", Bytes::from("test")).unwrap();
        let limits = ProcessingLimits::high_performance();
        let context = ProcessingContext::new(&message).with_limits(limits);

        assert_eq!(context.limits.max_processing_time, Some(Duration::from_millis(100)));
        assert_eq!(context.limits.max_plugin_chain_length, 10);
    }

    #[test]
    fn test_shared_metadata() {
        let message = Message::new("test.topic", Bytes::from("test")).unwrap();
        let context = ProcessingContext::new(&message);

        context.set_shared_metadata("key1".to_string(), "value1".to_string());
        context.set_shared_metadata("key2".to_string(), "value2".to_string());

        assert_eq!(context.get_shared_metadata("key1"), Some("value1".to_string()));
        assert_eq!(context.get_shared_metadata("key2"), Some("value2".to_string()));
        assert_eq!(context.get_shared_metadata("key3"), None);
    }

    #[test]
    fn test_metrics_context() {
        let mut metrics = MetricsContext::new();

        metrics.record_metric("latency".to_string(), 123.45);
        metrics.record_metric("throughput".to_string(), 1000.0);

        assert_eq!(metrics.custom_metrics.get("latency"), Some(&123.45));
        assert_eq!(metrics.custom_metrics.get("throughput"), Some(&1000.0));

        metrics.record_stage("start".to_string());
        std::thread::sleep(Duration::from_millis(1));
        metrics.record_stage("end".to_string());

        let duration = metrics.stage_duration("start", "end");
        assert!(duration.is_some());
        assert!(duration.unwrap() > Duration::ZERO);
    }

    #[test]
    fn test_plugin_chain_context() {
        let mut chain = PluginChainContext::new();
        chain.total_plugins = 3;

        assert!(chain.is_first_plugin());
        assert!(!chain.is_last_plugin());
        assert_eq!(chain.completion_percentage(), 0.0);

        let plugin_id = crate::plugin::PluginId::new();
        chain.record_plugin_execution(plugin_id);

        assert!(chain.is_first_plugin());
        assert!(!chain.is_last_plugin());
        assert!((chain.completion_percentage() - 33.333333333333336).abs() < f64::EPSILON);
    }

    #[test]
    fn test_processing_limits() {
        let limits = ProcessingLimits::default();
        assert_eq!(limits.max_processing_time, Some(Duration::from_secs(30)));
        assert_eq!(limits.max_plugin_chain_length, 100);

        let perf_limits = ProcessingLimits::high_performance();
        assert_eq!(perf_limits.max_processing_time, Some(Duration::from_millis(100)));
        assert_eq!(perf_limits.max_plugin_chain_length, 10);

        let dev_limits = ProcessingLimits::development();
        assert_eq!(dev_limits.max_processing_time, Some(Duration::from_secs(300)));
        assert_eq!(dev_limits.max_plugin_chain_length, 1000);

        let unlimited = ProcessingLimits::unlimited();
        assert_eq!(unlimited.max_processing_time, None);
        assert_eq!(unlimited.max_plugin_chain_length, usize::MAX);
    }

    #[test]
    fn test_tracing_context() {
        let mut tracing = TracingContext::new();
        assert!(!tracing.has_trace_info());

        tracing = TracingContext::with_ids(
            "trace-123".to_string(),
            "span-456".to_string(),
            Some("parent-789".to_string()),
        );

        assert!(tracing.has_trace_info());
        assert_eq!(tracing.trace_id, Some("trace-123".to_string()));
        assert_eq!(tracing.span_id, Some("span-456".to_string()));

        tracing.add_baggage("user_id".to_string(), "12345".to_string());
        assert_eq!(tracing.get_baggage("user_id"), Some("12345"));

        let child_span_id = tracing.new_child_span_id();
        assert!(!child_span_id.is_empty());
    }

    #[test]
    fn test_plugin_context() {
        let plugin_id = crate::plugin::PluginId::new();
        let mut context = PluginContext::new(
            plugin_id,
            "test-plugin".to_string(),
            semver::Version::new(1, 0, 0),
        );

        assert_eq!(context.plugin_name, "test-plugin");
        assert_eq!(context.plugin_version, semver::Version::new(1, 0, 0));

        context.set_config("setting1".to_string(), "value1".to_string());
        assert_eq!(context.get_config("setting1"), Some("value1"));
        assert_eq!(context.get_config("setting2"), None);

        assert!(context.uptime() > Duration::ZERO);
    }
}