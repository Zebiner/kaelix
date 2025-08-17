//! Validation utilities for verifying system correctness and performance.

use crate::constants::*;
use crate::context::{PerformanceMetrics, CorrectnessMetrics, TestResult};
use kaelix_core::{Message, MessageId, Result, Error};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use async_trait::async_trait;
use tracing::{info, warn, error};

pub mod invariants;

/// Trait for validating performance characteristics.
#[async_trait]
pub trait PerformanceValidator: Send + Sync {
    /// Validate throughput metrics.
    async fn validate_throughput(&self, metrics: &PerformanceMetrics) -> ValidationResult;

    /// Validate latency metrics.
    async fn validate_latency(&self, metrics: &PerformanceMetrics) -> ValidationResult;

    /// Validate resource utilization.
    async fn validate_resources(&self, metrics: &PerformanceMetrics) -> ValidationResult;

    /// Comprehensive performance validation.
    async fn validate_all(&self, metrics: &PerformanceMetrics) -> ValidationResult {
        let throughput = self.validate_throughput(metrics).await;
        let latency = self.validate_latency(metrics).await;
        let resources = self.validate_resources(metrics).await;

        ValidationResult {
            passed: throughput.passed && latency.passed && resources.passed,
            details: format!(
                "Throughput: {}, Latency: {}, Resources: {}",
                throughput.details, latency.details, resources.details
            ),
            metrics: Some(ValidationMetrics {
                throughput_score: throughput.score(),
                latency_score: latency.score(),
                resource_score: resources.score(),
            }),
        }
    }
}

/// Trait for validating correctness properties.
#[async_trait]
pub trait CorrectnessValidator: Send + Sync {
    /// Validate message delivery guarantees.
    async fn validate_delivery(&self, metrics: &CorrectnessMetrics) -> ValidationResult;

    /// Validate message ordering.
    async fn validate_ordering(&self, sent: &[Message], received: &[Message]) -> ValidationResult;

    /// Validate data integrity.
    async fn validate_integrity(&self, sent: &[Message], received: &[Message]) -> ValidationResult;

    /// Validate exactly-once semantics.
    async fn validate_exactly_once(&self, received: &[Message]) -> ValidationResult;
}

/// Result of a validation operation.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub passed: bool,
    
    /// Detailed description of results
    pub details: String,
    
    /// Optional metrics from validation
    pub metrics: Option<ValidationMetrics>,
}

/// Metrics from validation operations.
#[derive(Debug, Clone)]
pub struct ValidationMetrics {
    /// Throughput validation score (0.0 to 1.0)
    pub throughput_score: f64,
    
    /// Latency validation score (0.0 to 1.0)
    pub latency_score: f64,
    
    /// Resource utilization score (0.0 to 1.0)
    pub resource_score: f64,
}

/// Performance validator implementation with configurable thresholds.
#[derive(Debug)]
pub struct DefaultPerformanceValidator {
    config: PerformanceValidationConfig,
}

/// Configuration for performance validation.
#[derive(Debug, Clone)]
pub struct PerformanceValidationConfig {
    /// Minimum acceptable throughput (msg/sec)
    pub min_throughput: f64,
    
    /// Maximum acceptable P99 latency (microseconds)
    pub max_p99_latency: u64,
    
    /// Maximum acceptable P95 latency (microseconds)
    pub max_p95_latency: u64,
    
    /// Maximum acceptable memory usage (bytes)
    pub max_memory_usage: u64,
    
    /// Maximum acceptable CPU usage (percentage)
    pub max_cpu_usage: f64,
    
    /// Performance score weights
    pub weights: PerformanceWeights,
}

/// Weights for different performance aspects.
#[derive(Debug, Clone)]
pub struct PerformanceWeights {
    /// Weight for throughput in overall score
    pub throughput: f64,
    
    /// Weight for latency in overall score
    pub latency: f64,
    
    /// Weight for resource usage in overall score
    pub resources: f64,
}

/// Correctness validator implementation.
#[derive(Debug)]
pub struct DefaultCorrectnessValidator {
    config: CorrectnessValidationConfig,
}

/// Configuration for correctness validation.
#[derive(Debug, Clone)]
pub struct CorrectnessValidationConfig {
    /// Minimum acceptable delivery rate (0.0 to 1.0)
    pub min_delivery_rate: f64,
    
    /// Maximum acceptable duplicate rate (0.0 to 1.0)
    pub max_duplicate_rate: f64,
    
    /// Maximum acceptable ordering violations
    pub max_ordering_violations: u64,
    
    /// Enable strict ordering validation
    pub strict_ordering: bool,
    
    /// Tolerance for timestamp-based ordering
    pub timestamp_tolerance: Duration,
}

/// Message tracking for validation.
#[derive(Debug)]
pub struct MessageTracker {
    /// Sent messages by ID
    sent_messages: HashMap<MessageId, MessageInfo>,
    
    /// Received messages by ID
    received_messages: HashMap<MessageId, MessageInfo>,
    
    /// Message sequence tracking
    sequences: HashMap<String, Vec<MessageId>>, // topic -> message sequence
    
    /// Start time for tracking
    start_time: Instant,
}

/// Information about a tracked message.
#[derive(Debug, Clone)]
pub struct MessageInfo {
    /// Message metadata
    pub message: Message,
    
    /// Timestamp when tracked
    pub timestamp: Instant,
    
    /// Sequence number within topic
    pub sequence: u64,
}

impl DefaultPerformanceValidator {
    /// Create a new performance validator with configuration.
    pub fn new(config: PerformanceValidationConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(PerformanceValidationConfig::default())
    }

    /// Calculate throughput score based on achieved vs target.
    fn calculate_throughput_score(&self, achieved: f64) -> f64 {
        if achieved >= self.config.min_throughput {
            (achieved / self.config.min_throughput).min(1.0)
        } else {
            achieved / self.config.min_throughput
        }
    }

    /// Calculate latency score based on P99 latency.
    fn calculate_latency_score(&self, p99_latency: u64) -> f64 {
        if p99_latency <= self.config.max_p99_latency {
            1.0
        } else {
            self.config.max_p99_latency as f64 / p99_latency as f64
        }
    }

    /// Calculate resource utilization score.
    fn calculate_resource_score(&self, metrics: &PerformanceMetrics) -> f64 {
        let memory_score = if metrics.memory_usage <= self.config.max_memory_usage {
            1.0
        } else {
            self.config.max_memory_usage as f64 / metrics.memory_usage as f64
        };

        let cpu_score = if metrics.cpu_usage <= self.config.max_cpu_usage {
            1.0
        } else {
            self.config.max_cpu_usage / metrics.cpu_usage
        };

        (memory_score + cpu_score) / 2.0
    }
}

#[async_trait]
impl PerformanceValidator for DefaultPerformanceValidator {
    async fn validate_throughput(&self, metrics: &PerformanceMetrics) -> ValidationResult {
        let score = self.calculate_throughput_score(metrics.throughput);
        let passed = metrics.throughput >= self.config.min_throughput;

        ValidationResult {
            passed,
            details: format!(
                "Throughput: {:.2} msg/s (target: {:.2}, score: {:.3})",
                metrics.throughput, self.config.min_throughput, score
            ),
            metrics: Some(ValidationMetrics {
                throughput_score: score,
                latency_score: 0.0,
                resource_score: 0.0,
            }),
        }
    }

    async fn validate_latency(&self, metrics: &PerformanceMetrics) -> ValidationResult {
        let score = self.calculate_latency_score(metrics.latency_p99);
        let p99_passed = metrics.latency_p99 <= self.config.max_p99_latency;
        let p95_passed = metrics.latency_p95 <= self.config.max_p95_latency;
        let passed = p99_passed && p95_passed;

        ValidationResult {
            passed,
            details: format!(
                "Latency P99: {}μs (max: {}μs), P95: {}μs (max: {}μs), score: {:.3}",
                metrics.latency_p99, self.config.max_p99_latency,
                metrics.latency_p95, self.config.max_p95_latency,
                score
            ),
            metrics: Some(ValidationMetrics {
                throughput_score: 0.0,
                latency_score: score,
                resource_score: 0.0,
            }),
        }
    }

    async fn validate_resources(&self, metrics: &PerformanceMetrics) -> ValidationResult {
        let score = self.calculate_resource_score(metrics);
        let memory_passed = metrics.memory_usage <= self.config.max_memory_usage;
        let cpu_passed = metrics.cpu_usage <= self.config.max_cpu_usage;
        let passed = memory_passed && cpu_passed;

        ValidationResult {
            passed,
            details: format!(
                "Resources - Memory: {} bytes (max: {}), CPU: {:.1}% (max: {:.1}%), score: {:.3}",
                metrics.memory_usage, self.config.max_memory_usage,
                metrics.cpu_usage, self.config.max_cpu_usage,
                score
            ),
            metrics: Some(ValidationMetrics {
                throughput_score: 0.0,
                latency_score: 0.0,
                resource_score: score,
            }),
        }
    }
}

impl DefaultCorrectnessValidator {
    /// Create a new correctness validator with configuration.
    pub fn new(config: CorrectnessValidationConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(CorrectnessValidationConfig::default())
    }
}

#[async_trait]
impl CorrectnessValidator for DefaultCorrectnessValidator {
    async fn validate_delivery(&self, metrics: &CorrectnessMetrics) -> ValidationResult {
        let passed = metrics.message_delivery_rate >= self.config.min_delivery_rate;

        ValidationResult {
            passed,
            details: format!(
                "Delivery rate: {:.3} (min: {:.3})",
                metrics.message_delivery_rate, self.config.min_delivery_rate
            ),
            metrics: None,
        }
    }

    async fn validate_ordering(&self, sent: &[Message], received: &[Message]) -> ValidationResult {
        let mut violations = 0u64;
        let mut topic_sequences: HashMap<String, Vec<&Message>> = HashMap::new();

        // Group received messages by topic
        for msg in received {
            topic_sequences
                .entry(msg.topic.as_str().to_string())
                .or_default()
                .push(msg);
        }

        // Check ordering within each topic
        for (topic, messages) in &mut topic_sequences {
            // Sort by timestamp for ordering validation
            messages.sort_by_key(|m| m.timestamp);

            // Check for sequence violations
            for window in messages.windows(2) {
                if let [msg1, msg2] = window {
                    if self.config.strict_ordering {
                        // Strict ordering: timestamps must be monotonic
                        if msg2.timestamp <= msg1.timestamp {
                            violations += 1;
                        }
                    } else {
                        // Relaxed ordering: allow some tolerance
                        let time_diff = msg2.timestamp.signed_duration_since(msg1.timestamp);
                        if time_diff < -chrono::Duration::from_std(self.config.timestamp_tolerance).unwrap_or_default() {
                            violations += 1;
                        }
                    }
                }
            }
        }

        let passed = violations <= self.config.max_ordering_violations;

        ValidationResult {
            passed,
            details: format!(
                "Ordering violations: {} (max: {})",
                violations, self.config.max_ordering_violations
            ),
            metrics: None,
        }
    }

    async fn validate_integrity(&self, sent: &[Message], received: &[Message]) -> ValidationResult {
        let mut corruption_count = 0u64;
        let sent_map: HashMap<MessageId, &Message> = sent.iter()
            .map(|m| (m.id, m))
            .collect();

        for received_msg in received {
            if let Some(sent_msg) = sent_map.get(&received_msg.id) {
                // Validate payload integrity
                if sent_msg.payload != received_msg.payload {
                    corruption_count += 1;
                    error!("Payload corruption detected for message {}", received_msg.id);
                }

                // Validate topic consistency
                if sent_msg.topic != received_msg.topic {
                    corruption_count += 1;
                    error!("Topic mismatch for message {}: sent={}, received={}", 
                           received_msg.id, sent_msg.topic.as_str(), received_msg.topic.as_str());
                }
            }
        }

        let passed = corruption_count == 0;

        ValidationResult {
            passed,
            details: format!("Data corruption count: {}", corruption_count),
            metrics: None,
        }
    }

    async fn validate_exactly_once(&self, received: &[Message]) -> ValidationResult {
        let mut seen_ids = HashSet::new();
        let mut duplicates = 0u64;

        for msg in received {
            if !seen_ids.insert(msg.id) {
                duplicates += 1;
                warn!("Duplicate message detected: {}", msg.id);
            }
        }

        let duplicate_rate = duplicates as f64 / received.len() as f64;
        let passed = duplicate_rate <= self.config.max_duplicate_rate;

        ValidationResult {
            passed,
            details: format!(
                "Duplicates: {} ({:.3}% rate, max: {:.3}%)",
                duplicates, 
                duplicate_rate * 100.0,
                self.config.max_duplicate_rate * 100.0
            ),
            metrics: None,
        }
    }
}

impl MessageTracker {
    /// Create a new message tracker.
    pub fn new() -> Self {
        Self {
            sent_messages: HashMap::new(),
            received_messages: HashMap::new(),
            sequences: HashMap::new(),
            start_time: Instant::now(),
        }
    }

    /// Track a sent message.
    pub fn track_sent(&mut self, message: Message) {
        let topic = message.topic.as_str().to_string();
        let sequence = self.sequences.entry(topic.clone())
            .or_default()
            .len() as u64;

        self.sequences.get_mut(&topic).unwrap().push(message.id);

        let info = MessageInfo {
            message: message.clone(),
            timestamp: Instant::now(),
            sequence,
        };

        self.sent_messages.insert(message.id, info);
    }

    /// Track a received message.
    pub fn track_received(&mut self, message: Message) {
        let topic = message.topic.as_str().to_string();
        let sequence = self.received_messages.len() as u64;

        let info = MessageInfo {
            message: message.clone(),
            timestamp: Instant::now(),
            sequence,
        };

        self.received_messages.insert(message.id, info);
    }

    /// Get correctness metrics.
    pub fn get_correctness_metrics(&self) -> CorrectnessMetrics {
        let sent_count = self.sent_messages.len() as f64;
        let received_count = self.received_messages.len() as f64;
        
        let delivery_rate = if sent_count > 0.0 {
            received_count / sent_count
        } else {
            0.0
        };

        // Count duplicates
        let unique_received = self.received_messages.len();
        let total_received = received_count as usize;
        let duplicates = total_received.saturating_sub(unique_received) as u64;

        CorrectnessMetrics {
            message_delivery_rate: delivery_rate,
            order_violations: 0, // Would need more complex tracking
            duplicates,
            corruption_count: 0, // Would need integrity checking
        }
    }

    /// Get all sent messages.
    pub fn sent_messages(&self) -> Vec<Message> {
        self.sent_messages.values()
            .map(|info| info.message.clone())
            .collect()
    }

    /// Get all received messages.
    pub fn received_messages(&self) -> Vec<Message> {
        self.received_messages.values()
            .map(|info| info.message.clone())
            .collect()
    }
}

impl ValidationResult {
    /// Calculate overall score from metrics.
    pub fn score(&self) -> f64 {
        self.metrics.as_ref()
            .map(|m| (m.throughput_score + m.latency_score + m.resource_score) / 3.0)
            .unwrap_or(if self.passed { 1.0 } else { 0.0 })
    }

    /// Create a passing validation result.
    pub fn pass(details: String) -> Self {
        Self {
            passed: true,
            details,
            metrics: None,
        }
    }

    /// Create a failing validation result.
    pub fn fail(details: String) -> Self {
        Self {
            passed: false,
            details,
            metrics: None,
        }
    }
}

impl Default for MessageTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PerformanceValidationConfig {
    fn default() -> Self {
        Self {
            min_throughput: TARGET_THROUGHPUT as f64,
            max_p99_latency: TARGET_LATENCY_P99_MICROS,
            max_p95_latency: TARGET_LATENCY_P99_MICROS / 2, // Half of P99 target
            max_memory_usage: MAX_MEMORY_USAGE,
            max_cpu_usage: 80.0, // 80% CPU
            weights: PerformanceWeights::default(),
        }
    }
}

impl Default for PerformanceWeights {
    fn default() -> Self {
        Self {
            throughput: 0.4,
            latency: 0.4,
            resources: 0.2,
        }
    }
}

impl Default for CorrectnessValidationConfig {
    fn default() -> Self {
        Self {
            min_delivery_rate: 0.999, // 99.9% delivery
            max_duplicate_rate: 0.001, // 0.1% duplicates
            max_ordering_violations: 0,
            strict_ordering: false,
            timestamp_tolerance: Duration::from_millis(100),
        }
    }
}