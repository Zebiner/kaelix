//! Comprehensive data generators for test scenarios.
//!
//! This module provides high-performance message and data generation
//! supporting various distributions, patterns, and workload types.

use crate::constants::*;
use kaelix_core::{Message, Topic, Result, Error};
use bytes::Bytes;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::time::Duration;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use rand::Rng;
use rand_distr::{Distribution, Normal, Exponential, Pareto};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use tokio_stream::wrappers::IntervalStream;
use tokio::time::{interval, Interval};

/// Payload distribution strategies for realistic data generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PayloadDistribution {
    /// Uniform distribution between min and max bytes
    Uniform { min: usize, max: usize },
    /// Normal distribution with mean and standard deviation
    Normal { mean: f64, std_dev: f64 },
    /// Exponential distribution with lambda parameter
    Exponential { lambda: f64 },
    /// Pareto distribution for heavy-tailed scenarios
    Pareto { scale: f64, shape: f64 },
    /// Realistic distribution based on production patterns
    Realistic,
}

/// Key distribution strategies for message routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyDistribution {
    /// No keys (null partitioning)
    None,
    /// Random keys with uniform distribution
    Random,
    /// Sequential keys for ordered processing
    Sequential,
    /// Zipf distribution for hotspot simulation
    Zipf { alpha: f64 },
    /// Custom key patterns
    Pattern(String),
}

/// Compression types for payload generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionType {
    /// No compression
    None,
    /// Gzip compression
    Gzip,
    /// LZ4 compression (fast)
    Lz4,
    /// Zstd compression (high ratio)
    Zstd,
    /// Snappy compression (fast)
    Snappy,
}

/// High-performance message generator with various configurations
#[derive(Debug)]
pub struct MessageGenerator {
    /// Payload size distribution strategy
    distribution: PayloadDistribution,
    /// Range of payload sizes
    payload_sizes: std::ops::Range<usize>,
    /// Topic patterns for generation
    topic_patterns: Vec<String>,
    /// Key distribution strategy
    key_distribution: KeyDistribution,
    /// Optional compression
    compression: Option<CompressionType>,
    /// Message counter for tracking
    counter: Arc<AtomicU64>,
    /// Generation statistics
    stats: Arc<parking_lot::RwLock<GenerationStats>>,
}

/// Statistics for message generation performance
#[derive(Debug, Clone, Default)]
pub struct GenerationStats {
    /// Total messages generated
    pub messages_generated: u64,
    /// Total bytes generated
    pub bytes_generated: u64,
    /// Generation duration
    pub duration: Duration,
    /// Average generation rate (msg/sec)
    pub avg_rate: f64,
    /// Average message size
    pub avg_message_size: f64,
    /// Peak generation rate
    pub peak_rate: f64,
    /// Compression ratio (if enabled)
    pub compression_ratio: Option<f64>,
}

impl MessageGenerator {
    /// Create a new message generator with default settings
    pub fn new() -> Self {
        Self {
            distribution: PayloadDistribution::Uniform { min: 100, max: 1024 },
            payload_sizes: 100..1024,
            topic_patterns: vec!["test-topic".to_string()],
            key_distribution: KeyDistribution::Random,
            compression: None,
            counter: Arc::new(AtomicU64::new(0)),
            stats: Arc::new(parking_lot::RwLock::new(GenerationStats::default())),
        }
    }

    /// Configure for realistic production workloads
    pub fn realistic_workload() -> Self {
        Self {
            distribution: PayloadDistribution::Realistic,
            payload_sizes: 64..65536,
            topic_patterns: vec![
                "events.user.{user_id}".to_string(),
                "metrics.system.{metric_type}".to_string(),
                "logs.application.{service}".to_string(),
                "notifications.{channel}".to_string(),
            ],
            key_distribution: KeyDistribution::Zipf { alpha: 1.1 },
            compression: Some(CompressionType::Lz4),
            counter: Arc::new(AtomicU64::new(0)),
            stats: Arc::new(parking_lot::RwLock::new(GenerationStats::default())),
        }
    }

    /// Configure for stress testing scenarios
    pub fn stress_test() -> Self {
        Self {
            distribution: PayloadDistribution::Uniform { min: 1024, max: 4096 },
            payload_sizes: 1024..4096,
            topic_patterns: vec!["stress-test".to_string()],
            key_distribution: KeyDistribution::Random,
            compression: None,
            counter: Arc::new(AtomicU64::new(0)),
            stats: Arc::new(parking_lot::RwLock::new(GenerationStats::default())),
        }
    }

    /// Configure for microbenchmarking
    pub fn micro_benchmark() -> Self {
        Self {
            distribution: PayloadDistribution::Uniform { min: 64, max: 64 },
            payload_sizes: 64..65,
            topic_patterns: vec!["benchmark".to_string()],
            key_distribution: KeyDistribution::Sequential,
            compression: None,
            counter: Arc::new(AtomicU64::new(0)),
            stats: Arc::new(parking_lot::RwLock::new(GenerationStats::default())),
        }
    }

    /// Configure for large payload testing
    pub fn large_payload() -> Self {
        Self {
            distribution: PayloadDistribution::Normal { mean: 1048576.0, std_dev: 262144.0 }, // 1MB ± 256KB
            payload_sizes: 65536..2097152, // 64KB to 2MB
            topic_patterns: vec!["large-data".to_string()],
            key_distribution: KeyDistribution::Random,
            compression: Some(CompressionType::Zstd),
            counter: Arc::new(AtomicU64::new(0)),
            stats: Arc::new(parking_lot::RwLock::new(GenerationStats::default())),
        }
    }

    /// Generate a stream of messages at specified rate
    pub async fn generate_stream(&self, rate: u64) -> impl Stream<Item = Message> + '_ {
        let interval_duration = Duration::from_nanos(1_000_000_000 / rate.max(1));
        let mut interval = interval(interval_duration);
        
        async_stream::stream! {
            loop {
                interval.tick().await;
                if let Ok(message) = self.generate_single().await {
                    yield message;
                }
            }
        }
    }

    /// Generate a batch of messages
    pub async fn generate_batch(&self, count: usize) -> Result<Vec<Message>> {
        let mut messages = Vec::with_capacity(count);
        let start = std::time::Instant::now();

        for _ in 0..count {
            messages.push(self.generate_single().await?);
        }

        // Update statistics
        let elapsed = start.elapsed();
        let mut stats = self.stats.write();
        stats.messages_generated += count as u64;
        stats.bytes_generated += messages.iter().map(|m| m.payload().len() as u64).sum::<u64>();
        stats.duration += elapsed;
        stats.avg_rate = stats.messages_generated as f64 / stats.duration.as_secs_f64();

        Ok(messages)
    }

    /// Generate a single message based on configuration
    async fn generate_single(&self) -> Result<Message> {
        let size = self.sample_payload_size();
        let payload = self.generate_payload(size).await?;
        let topic = self.select_topic().await?;
        let key = self.generate_key().await;

        let mut message = Message::new(topic.as_str(), payload)?;
        
        // Set key if generated
        if let Some(key) = key {
            message.set_key(key)?;
        }

        // Apply compression if configured
        if let Some(compression) = &self.compression {
            message = self.apply_compression(message, compression).await?;
        }

        self.counter.fetch_add(1, Ordering::Relaxed);
        Ok(message)
    }

    /// Sample payload size based on distribution
    fn sample_payload_size(&self) -> usize {
        match &self.distribution {
            PayloadDistribution::Uniform { min, max } => {
                rand::thread_rng().gen_range(*min..=*max)
            }
            PayloadDistribution::Normal { mean, std_dev } => {
                let normal = Normal::new(*mean, *std_dev).unwrap_or_else(|_| {
                    Normal::new(1024.0, 256.0).unwrap()
                });
                let size = normal.sample(&mut rand::thread_rng()).max(1.0) as usize;
                size.clamp(self.payload_sizes.start, self.payload_sizes.end)
            }
            PayloadDistribution::Exponential { lambda } => {
                let exp = Exponential::new(*lambda).unwrap_or_else(|_| {
                    Exponential::new(0.001).unwrap()
                });
                let size = exp.sample(&mut rand::thread_rng()) as usize;
                size.clamp(self.payload_sizes.start, self.payload_sizes.end)
            }
            PayloadDistribution::Pareto { scale, shape } => {
                let pareto = Pareto::new(*scale, *shape).unwrap_or_else(|_| {
                    Pareto::new(100.0, 1.16).unwrap()
                });
                let size = pareto.sample(&mut rand::thread_rng()) as usize;
                size.clamp(self.payload_sizes.start, self.payload_sizes.end)
            }
            PayloadDistribution::Realistic => {
                // Realistic distribution: 70% small, 25% medium, 5% large
                let r: f64 = rand::thread_rng().gen();
                if r < 0.7 {
                    rand::thread_rng().gen_range(64..=512) // Small messages
                } else if r < 0.95 {
                    rand::thread_rng().gen_range(512..=4096) // Medium messages
                } else {
                    rand::thread_rng().gen_range(4096..=65536) // Large messages
                }
            }
        }
    }

    /// Generate payload data of specified size
    async fn generate_payload(&self, size: usize) -> Result<Bytes> {
        // Use high-performance generation for large payloads
        if size > 65536 {
            // For large payloads, use patterns to reduce memory allocation
            let pattern = b"0123456789ABCDEF";
            let pattern_len = pattern.len();
            let mut payload = Vec::with_capacity(size);
            
            for i in 0..size {
                payload.push(pattern[i % pattern_len]);
            }
            
            Ok(Bytes::from(payload))
        } else {
            // For smaller payloads, use random data
            let mut payload = vec![0u8; size];
            rand::thread_rng().fill(&mut payload[..]);
            Ok(Bytes::from(payload))
        }
    }

    /// Select topic based on patterns
    async fn select_topic(&self) -> Result<Topic> {
        if self.topic_patterns.is_empty() {
            return Err(Error::Configuration {
                message: "No topic patterns configured".to_string(),
            });
        }

        let pattern = &self.topic_patterns[rand::thread_rng().gen_range(0..self.topic_patterns.len())];
        let topic_name = self.expand_topic_pattern(pattern).await;
        Topic::new(topic_name)
    }

    /// Expand topic patterns with variables
    async fn expand_topic_pattern(&self, pattern: &str) -> String {
        let mut result = pattern.to_string();
        
        // Replace common patterns
        result = result.replace("{user_id}", &rand::thread_rng().gen_range(1..=10000).to_string());
        result = result.replace("{metric_type}", &["cpu", "memory", "disk", "network"][rand::thread_rng().gen_range(0..4)]);
        result = result.replace("{service}", &["api", "web", "worker", "db"][rand::thread_rng().gen_range(0..4)]);
        result = result.replace("{channel}", &["email", "sms", "push", "webhook"][rand::thread_rng().gen_range(0..4)]);
        
        result
    }

    /// Generate message key based on distribution
    async fn generate_key(&self) -> Option<String> {
        match &self.key_distribution {
            KeyDistribution::None => None,
            KeyDistribution::Random => {
                Some(Uuid::new_v4().to_string())
            }
            KeyDistribution::Sequential => {
                let seq = self.counter.load(Ordering::Relaxed);
                Some(format!("key-{:08x}", seq))
            }
            KeyDistribution::Zipf { alpha } => {
                // Simplified Zipf distribution implementation
                let rank = self.zipf_sample(*alpha);
                Some(format!("hotkey-{}", rank))
            }
            KeyDistribution::Pattern(pattern) => {
                Some(pattern.replace("{id}", &rand::thread_rng().gen_range(1..=1000).to_string()))
            }
        }
    }

    /// Sample from Zipf distribution (simplified implementation)
    fn zipf_sample(&self, alpha: f64) -> u32 {
        // Simplified Zipf sampling - in production, use a proper implementation
        let u: f64 = rand::thread_rng().gen();
        let n = 1000; // Number of possible keys
        let h_n = (1..=n).map(|i| 1.0 / (i as f64).powf(alpha)).sum::<f64>();
        let target = u * h_n;
        
        let mut sum = 0.0;
        for i in 1..=n {
            sum += 1.0 / (i as f64).powf(alpha);
            if sum >= target {
                return i;
            }
        }
        n
    }

    /// Apply compression to message
    async fn apply_compression(&self, mut message: Message, compression: &CompressionType) -> Result<Message> {
        let original_size = message.payload().len();
        let compressed_payload = match compression {
            CompressionType::None => return Ok(message),
            CompressionType::Gzip => {
                use flate2::Compression;
                use flate2::write::GzEncoder;
                use std::io::Write;
                
                let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
                encoder.write_all(message.payload())?;
                Bytes::from(encoder.finish()?)
            }
            CompressionType::Lz4 => {
                // Note: Would need lz4_flex crate in real implementation
                // For now, simulate compression by reducing size slightly
                let data = message.payload();
                let compressed_size = (data.len() as f64 * 0.7) as usize; // Simulate 30% compression
                Bytes::from(data[..compressed_size.min(data.len())].to_vec())
            }
            CompressionType::Zstd => {
                // Note: Would need zstd crate in real implementation
                // Simulate high compression ratio
                let data = message.payload();
                let compressed_size = (data.len() as f64 * 0.5) as usize; // Simulate 50% compression
                Bytes::from(data[..compressed_size.min(data.len())].to_vec())
            }
            CompressionType::Snappy => {
                // Note: Would need snap crate in real implementation
                // Simulate fast compression
                let data = message.payload();
                let compressed_size = (data.len() as f64 * 0.8) as usize; // Simulate 20% compression
                Bytes::from(data[..compressed_size.min(data.len())].to_vec())
            }
        };

        // Update compression ratio in stats
        let compressed_size = compressed_payload.len();
        if compressed_size > 0 && original_size > 0 {
            let mut stats = self.stats.write();
            let ratio = compressed_size as f64 / original_size as f64;
            stats.compression_ratio = Some(
                stats.compression_ratio.map_or(ratio, |existing| (existing + ratio) / 2.0)
            );
        }

        message.set_payload(compressed_payload)?;
        Ok(message)
    }

    /// Get current generation statistics
    pub fn stats(&self) -> GenerationStats {
        self.stats.read().clone()
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        let mut stats = self.stats.write();
        *stats = GenerationStats::default();
        self.counter.store(0, Ordering::Relaxed);
    }

    /// Set payload distribution
    pub fn with_distribution(mut self, distribution: PayloadDistribution) -> Self {
        self.distribution = distribution;
        self
    }

    /// Set topic patterns
    pub fn with_topic_patterns(mut self, patterns: Vec<String>) -> Self {
        self.topic_patterns = patterns;
        self
    }

    /// Set key distribution
    pub fn with_key_distribution(mut self, distribution: KeyDistribution) -> Self {
        self.key_distribution = distribution;
        self
    }

    /// Set compression type
    pub fn with_compression(mut self, compression: CompressionType) -> Self {
        self.compression = Some(compression);
        self
    }
}

impl Default for MessageGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream-based message generation for memory efficiency
pub struct StreamingMessageGenerator {
    generator: MessageGenerator,
    rate: u64,
    burst_config: Option<BurstConfig>,
}

/// Configuration for burst generation
#[derive(Debug, Clone)]
pub struct BurstConfig {
    /// Base rate (messages per second)
    pub base_rate: u64,
    /// Peak rate during burst
    pub peak_rate: u64,
    /// Burst duration
    pub burst_duration: Duration,
    /// Interval between bursts
    pub burst_interval: Duration,
}

impl StreamingMessageGenerator {
    /// Create new streaming generator
    pub fn new(generator: MessageGenerator, rate: u64) -> Self {
        Self {
            generator,
            rate,
            burst_config: None,
        }
    }

    /// Configure burst generation
    pub fn with_bursts(mut self, config: BurstConfig) -> Self {
        self.burst_config = Some(config);
        self
    }

    /// Generate infinite stream of messages
    pub fn stream(&self) -> impl Stream<Item = Result<Message>> + '_ {
        async_stream::stream! {
            if let Some(burst_config) = &self.burst_config {
                yield_all!(self.burst_stream(burst_config));
            } else {
                yield_all!(self.constant_stream());
            }
        }
    }

    /// Generate constant rate stream
    fn constant_stream(&self) -> impl Stream<Item = Result<Message>> + '_ {
        let interval_duration = Duration::from_nanos(1_000_000_000 / self.rate.max(1));
        
        async_stream::stream! {
            let mut interval = interval(interval_duration);
            loop {
                interval.tick().await;
                yield self.generator.generate_single().await;
            }
        }
    }

    /// Generate burst pattern stream
    fn burst_stream(&self, config: &BurstConfig) -> impl Stream<Item = Result<Message>> + '_ {
        async_stream::stream! {
            let mut burst_timer = interval(config.burst_interval);
            let mut is_bursting = false;
            let mut burst_start = std::time::Instant::now();

            loop {
                // Check if we should start/stop bursting
                if !is_bursting {
                    burst_timer.tick().await;
                    is_bursting = true;
                    burst_start = std::time::Instant::now();
                } else if burst_start.elapsed() >= config.burst_duration {
                    is_bursting = false;
                    continue;
                }

                // Generate at appropriate rate
                let current_rate = if is_bursting { config.peak_rate } else { config.base_rate };
                let interval_duration = Duration::from_nanos(1_000_000_000 / current_rate.max(1));
                
                tokio::time::sleep(interval_duration).await;
                yield self.generator.generate_single().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_message_generator_basic() {
        let generator = MessageGenerator::new();
        let message = generator.generate_single().await.unwrap();
        
        assert!(!message.topic().as_str().is_empty());
        assert!(!message.payload().is_empty());
    }

    #[tokio::test]
    async fn test_realistic_workload_generator() {
        let generator = MessageGenerator::realistic_workload();
        let batch = generator.generate_batch(100).await.unwrap();
        
        assert_eq!(batch.len(), 100);
        
        // Check that topics were expanded
        let topics: std::collections::HashSet<_> = batch.iter()
            .map(|m| m.topic().as_str())
            .collect();
        assert!(topics.len() > 1, "Should generate different topic names");
    }

    #[tokio::test]
    async fn test_stress_test_generator() {
        let generator = MessageGenerator::stress_test();
        let batch = generator.generate_batch(1000).await.unwrap();
        
        assert_eq!(batch.len(), 1000);
        
        // All messages should be within size range
        for message in &batch {
            let size = message.payload().len();
            assert!(size >= 1024 && size <= 4096);
        }
    }

    #[tokio::test]
    async fn test_streaming_generator() {
        let generator = MessageGenerator::micro_benchmark();
        let streaming = StreamingMessageGenerator::new(generator, 1000);
        
        let messages: Vec<_> = streaming.stream()
            .take(10)
            .collect()
            .await;
        
        assert_eq!(messages.len(), 10);
        assert!(messages.iter().all(|r| r.is_ok()));
    }

    #[tokio::test]
    async fn test_payload_distributions() {
        let distributions = vec![
            PayloadDistribution::Uniform { min: 100, max: 200 },
            PayloadDistribution::Normal { mean: 150.0, std_dev: 25.0 },
            PayloadDistribution::Realistic,
        ];

        for distribution in distributions {
            let generator = MessageGenerator::new().with_distribution(distribution);
            let message = generator.generate_single().await.unwrap();
            assert!(!message.payload().is_empty());
        }
    }

    #[tokio::test]
    async fn test_key_distributions() {
        let distributions = vec![
            KeyDistribution::None,
            KeyDistribution::Random,
            KeyDistribution::Sequential,
            KeyDistribution::Pattern("test-{id}".to_string()),
        ];

        for distribution in distributions {
            let generator = MessageGenerator::new().with_key_distribution(distribution);
            let message = generator.generate_single().await.unwrap();
            // Key presence depends on distribution type
        }
    }
}