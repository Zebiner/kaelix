//! Data generators for testing scenarios.

use crate::constants::*;
use kaelix_core::{Message, Topic, Result};
use bytes::Bytes;
use async_trait::async_trait;
use std::time::Duration;
use fake::{Fake, Faker};
use proptest::prelude::*;
use uuid::Uuid;
use std::sync::atomic::{AtomicU64, Ordering};
use rand_distr::{Distribution, Exp, Normal};

pub mod properties;
pub mod data;
pub mod load;
pub mod workload;
pub mod realistic;
pub mod efficient;

/// Trait for generating test data at specified rates and patterns.
#[async_trait]
pub trait LoadGenerator: Send + Sync {
    /// Generate a single message.
    async fn generate_message(&mut self) -> Result<Message>;

    /// Generate a batch of messages.
    async fn generate_batch(&mut self, count: usize) -> Result<Vec<Message>>;

    /// Generate messages at a specified rate for a duration.
    async fn generate_with_rate(
        &mut self,
        rate: u64,
        duration: Duration,
    ) -> Result<Vec<Message>>;

    /// Get the current generation statistics.
    fn stats(&self) -> GeneratorStats;
}

/// Statistics for message generation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GeneratorStats {
    /// Total messages generated
    pub total_generated: u64,
    /// Generation rate (messages per second)
    pub generation_rate: f64,
    /// Average message size
    pub avg_message_size: usize,
    /// Generation duration
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
}

/// Configurable message generator for various test scenarios.
#[derive(Debug)]
pub struct MessageGenerator {
    /// Target throughput (messages per second)
    throughput: u64,
    /// Message payload size distribution
    payload_size: PayloadSizeConfig,
    /// Topic distribution strategy
    topic_strategy: TopicStrategy,
    /// Message content generator
    content_generator: ContentGenerator,
    /// Statistics
    stats: GeneratorStats,
    /// Message counter
    counter: AtomicU64,
}

/// Configuration for message payload sizes.
#[derive(Debug, Clone)]
pub enum PayloadSizeConfig {
    /// Fixed size in bytes
    Fixed(usize),
    /// Random size between min and max
    Random { min: usize, max: usize },
    /// Exponential distribution with rate parameter
    Exponential { rate: f64 },
    /// Normal distribution with mean and standard deviation
    Normal { mean: f64, std_dev: f64 },
    /// Realistic distribution based on real-world data
    Realistic,
}

// Rest of the file remains the same
