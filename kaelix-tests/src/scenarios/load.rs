//! Load testing scenarios for comprehensive performance validation.
//!
//! This module provides predefined load testing scenarios that validate
//! system performance under various stress conditions.

use crate::generators::workload::{ProducerWorkload, ConsumerWorkload, WorkloadSimulator};
use crate::generators::data::{PayloadDistribution, CompressionType};
use crate::generators::load::{LoadPattern, LoadProfile};
use std::time::Duration;
use std::collections::HashMap;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

/// Load test scenario definition
#[derive(Debug, Clone)]
pub struct LoadTestScenario {
    /// Scenario name
    pub name: String,
    /// Scenario description
    pub description: String,
    /// Producer configurations
    pub producers: Vec<ProducerConfig>,
    /// Consumer configurations
    pub consumers: Vec<ConsumerConfig>,
    /// Test duration
    pub duration: Duration,
    /// Success criteria
    pub success_criteria: SuccessCriteria,
    /// Scenario metadata
    pub metadata: HashMap<String, String>,
}

/// Producer configuration for load testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerConfig {
    /// Producer identifier
    pub id: String,
    /// Number of producer instances
    pub instance_count: usize,
    /// Topics to produce to
    pub topics: Vec<String>,
    /// Target production rate (messages/sec per instance)
    pub target_rate: u64,
    /// Message payload configuration
    pub payload_config: PayloadConfig,
    /// Load pattern
    pub load_pattern: LoadPattern,
    /// Producer priority
    pub priority: u8,
}

/// Consumer configuration for load testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerConfig {
    /// Consumer identifier
    pub id: String,
    /// Number of consumer instances
    pub instance_count: usize,
    /// Topics to consume from
    pub topics: Vec<String>,
    /// Consumption rate limit (messages/sec per instance)
    pub rate_limit: Option<u64>,
    /// Batch size for consumption
    pub batch_size: usize,
    /// Processing delay simulation
    pub processing_delay: Duration,
    /// Consumer group
    pub consumer_group: String,
}

/// Payload configuration for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadConfig {
    /// Payload size distribution
    pub size_distribution: PayloadDistribution,
    /// Compression type
    pub compression: Option<CompressionType>,
    /// Content type
    pub content_type: ContentType,
}

/// Content type for payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
    /// Random binary data
    Random,
    /// JSON structured data
    Json,
    /// Text data
    Text,
    /// Protobuf-like binary
    Protobuf,
    /// Custom pattern
    Custom(String),
}

/// Success criteria for load test scenarios
#[derive(Debug, Clone)]
pub struct SuccessCriteria {
    /// Minimum throughput (messages/sec)
    pub min_throughput: u64,
    /// Maximum P99 latency
    pub max_p99_latency: Duration,
    /// Maximum error rate (0.0 to 1.0)
    pub max_error_rate: f64,
    /// Maximum memory usage (bytes)
    pub max_memory_usage: usize,
    /// Resource utilization limits
    pub resource_limits: ResourceLimits,
    /// Custom success criteria
    pub custom_criteria: HashMap<String, String>,
}

/// Resource utilization limits
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum CPU utilization (0.0 to 1.0)
    pub max_cpu_utilization: f64,
    /// Maximum memory utilization (0.0 to 1.0)
    pub max_memory_utilization: f64,
    /// Maximum network utilization (bytes/sec)
    pub max_network_utilization: u64,
    /// Maximum disk utilization (0.0 to 1.0)
    pub max_disk_utilization: f64,
}

impl LoadTestScenario {
    /// Standard throughput test - validate peak throughput
    pub fn standard_throughput() -> Self {
        Self {
            name: "standard_throughput".to_string(),
            description: "Validate system can achieve target throughput with acceptable latency".to_string(),
            producers: vec![
                ProducerConfig {
                    id: "high_throughput_producer".to_string(),
                    instance_count: 10,
                    topics: vec!["throughput.test".to_string()],
                    target_rate: 100_000, // 100K messages/sec per instance
                    payload_config: PayloadConfig {
                        size_distribution: PayloadDistribution::Uniform { min: 1024, max: 4096 },
                        compression: Some(CompressionType::Lz4),
                        content_type: ContentType::Random,
                    },
                    load_pattern: LoadPattern::Constant { rate: 100_000 },
                    priority: 1,
                },
            ],
            consumers: vec![
                ConsumerConfig {
                    id: "throughput_consumer".to_string(),
                    instance_count: 5,
                    topics: vec!["throughput.test".to_string()],
                    rate_limit: None, // No rate limit - consume as fast as possible
                    batch_size: 1000,
                    processing_delay: Duration::from_micros(10),
                    consumer_group: "throughput_test".to_string(),
                },
            ],
            duration: Duration::from_minutes(10),
            success_criteria: SuccessCriteria {
                min_throughput: 800_000, // 80% of target (10 * 100K * 0.8)
                max_p99_latency: Duration::from_micros(100),
                max_error_rate: 0.001,
                max_memory_usage: 8 * 1024 * 1024 * 1024, // 8GB
                resource_limits: ResourceLimits {
                    max_cpu_utilization: 0.8,
                    max_memory_utilization: 0.7,
                    max_network_utilization: 10 * 1024 * 1024 * 1024, // 10GB/sec
                    max_disk_utilization: 0.6,
                },
                custom_criteria: HashMap::new(),
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("test_type".to_string(), "throughput".to_string());
                meta.insert("target_rate".to_string(), "1M_messages_per_second".to_string());
                meta
            },
        }
    }

    /// Peak load test - maximum sustainable throughput
    pub fn peak_load() -> Self {
        Self {
            name: "peak_load".to_string(),
            description: "Test maximum sustainable load with degradation monitoring".to_string(),
            producers: vec![
                ProducerConfig {
                    id: "peak_producer".to_string(),
                    instance_count: 20,
                    topics: vec!["peak.load.test".to_string()],
                    target_rate: 500_000, // Aggressive rate
                    payload_config: PayloadConfig {
                        size_distribution: PayloadDistribution::Normal { mean: 2048.0, std_dev: 512.0 },
                        compression: Some(CompressionType::Zstd),
                        content_type: ContentType::Json,
                    },
                    load_pattern: LoadPattern::Sawtooth {
                        min: 100_000,
                        max: 500_000,
                        period: Duration::from_minutes(5),
                    },
                    priority: 1,
                },
            ],
            consumers: vec![
                ConsumerConfig {
                    id: "peak_consumer".to_string(),
                    instance_count: 15,
                    topics: vec!["peak.load.test".to_string()],
                    rate_limit: None,
                    batch_size: 5000,
                    processing_delay: Duration::from_micros(5),
                    consumer_group: "peak_load_test".to_string(),
                },
            ],
            duration: Duration::from_minutes(30),
            success_criteria: SuccessCriteria {
                min_throughput: 8_000_000, // 8M messages/sec
                max_p99_latency: Duration::from_millis(1),
                max_error_rate: 0.01, // Higher error rate acceptable under peak load
                max_memory_usage: 16 * 1024 * 1024 * 1024, // 16GB
                resource_limits: ResourceLimits {
                    max_cpu_utilization: 0.95,
                    max_memory_utilization: 0.85,
                    max_network_utilization: 50 * 1024 * 1024 * 1024, // 50GB/sec
                    max_disk_utilization: 0.8,
                },
                custom_criteria: {
                    let mut criteria = HashMap::new();
                    criteria.insert("degradation_threshold".to_string(), "20_percent".to_string());
                    criteria.insert("recovery_time".to_string(), "under_60_seconds".to_string());
                    criteria
                },
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("test_type".to_string(), "peak_load".to_string());
                meta.insert("stress_level".to_string(), "maximum".to_string());
                meta
            },
        }
    }

    /// Sustained load test - long-term stability
    pub fn sustained_load() -> Self {
        Self {
            name: "sustained_load".to_string(),
            description: "Long-term sustained load to test stability and resource management".to_string(),
            producers: vec![
                ProducerConfig {
                    id: "sustained_producer".to_string(),
                    instance_count: 8,
                    topics: vec!["sustained.test".to_string()],
                    target_rate: 50_000,
                    payload_config: PayloadConfig {
                        size_distribution: PayloadDistribution::Realistic,
                        compression: Some(CompressionType::Snappy),
                        content_type: ContentType::Json,
                    },
                    load_pattern: LoadPattern::Realistic { profile: LoadProfile::BusinessHours },
                    priority: 1,
                },
            ],
            consumers: vec![
                ConsumerConfig {
                    id: "sustained_consumer".to_string(),
                    instance_count: 6,
                    topics: vec!["sustained.test".to_string()],
                    rate_limit: Some(60_000), // Slight rate limit to maintain stability
                    batch_size: 500,
                    processing_delay: Duration::from_micros(20),
                    consumer_group: "sustained_test".to_string(),
                },
            ],
            duration: Duration::from_hours(4), // Long-term test
            success_criteria: SuccessCriteria {
                min_throughput: 320_000, // 8 * 50K * 0.8
                max_p99_latency: Duration::from_micros(50),
                max_error_rate: 0.0005,
                max_memory_usage: 4 * 1024 * 1024 * 1024, // 4GB
                resource_limits: ResourceLimits {
                    max_cpu_utilization: 0.6,
                    max_memory_utilization: 0.5,
                    max_network_utilization: 5 * 1024 * 1024 * 1024, // 5GB/sec
                    max_disk_utilization: 0.4,
                },
                custom_criteria: {
                    let mut criteria = HashMap::new();
                    criteria.insert("memory_leak_threshold".to_string(), "5_percent_growth".to_string());
                    criteria.insert("gc_pressure".to_string(), "acceptable".to_string());
                    criteria
                },
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("test_type".to_string(), "sustained".to_string());
                meta.insert("duration".to_string(), "4_hours".to_string());
                meta
            },
        }
    }

    /// Burst recovery test - recovery after traffic spikes
    pub fn burst_recovery() -> Self {
        Self {
            name: "burst_recovery".to_string(),
            description: "Test system recovery capabilities after traffic bursts".to_string(),
            producers: vec![
                ProducerConfig {
                    id: "burst_producer".to_string(),
                    instance_count: 15,
                    topics: vec!["burst.recovery.test".to_string()],
                    target_rate: 200_000,
                    payload_config: PayloadConfig {
                        size_distribution: PayloadDistribution::Pareto { scale: 512.0, shape: 1.5 },
                        compression: None, // No compression to increase load
                        content_type: ContentType::Random,
                    },
                    load_pattern: LoadPattern::Burst {
                        base: 10_000,
                        peak: 200_000,
                        duration: Duration::from_minutes(2),
                        interval: Duration::from_minutes(10),
                    },
                    priority: 1,
                },
            ],
            consumers: vec![
                ConsumerConfig {
                    id: "recovery_consumer".to_string(),
                    instance_count: 10,
                    topics: vec!["burst.recovery.test".to_string()],
                    rate_limit: Some(100_000),
                    batch_size: 2000,
                    processing_delay: Duration::from_micros(15),
                    consumer_group: "burst_recovery".to_string(),
                },
            ],
            duration: Duration::from_minutes(60),
            success_criteria: SuccessCriteria {
                min_throughput: 800_000, // Handle burst effectively
                max_p99_latency: Duration::from_millis(5), // Higher latency acceptable during bursts
                max_error_rate: 0.005,
                max_memory_usage: 12 * 1024 * 1024 * 1024, // 12GB
                resource_limits: ResourceLimits {
                    max_cpu_utilization: 0.9,
                    max_memory_utilization: 0.8,
                    max_network_utilization: 25 * 1024 * 1024 * 1024, // 25GB/sec
                    max_disk_utilization: 0.7,
                },
                custom_criteria: {
                    let mut criteria = HashMap::new();
                    criteria.insert("recovery_time".to_string(), "under_30_seconds".to_string());
                    criteria.insert("backpressure_handling".to_string(), "effective".to_string());
                    criteria.insert("queue_overflow".to_string(), "none".to_string());
                    criteria
                },
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("test_type".to_string(), "burst_recovery".to_string());
                meta.insert("burst_pattern".to_string(), "2min_peak_8min_recovery".to_string());
                meta
            },
        }
    }

    /// Mixed workload test - realistic production simulation
    pub fn mixed_workload() -> Self {
        Self {
            name: "mixed_workload".to_string(),
            description: "Mixed workload simulation with multiple producer/consumer patterns".to_string(),
            producers: vec![
                // High-frequency small messages
                ProducerConfig {
                    id: "micro_producer".to_string(),
                    instance_count: 5,
                    topics: vec!["micro.events".to_string()],
                    target_rate: 100_000,
                    payload_config: PayloadConfig {
                        size_distribution: PayloadDistribution::Uniform { min: 64, max: 256 },
                        compression: Some(CompressionType::Snappy),
                        content_type: ContentType::Json,
                    },
                    load_pattern: LoadPattern::Constant { rate: 100_000 },
                    priority: 2,
                },
                // Medium-frequency medium messages
                ProducerConfig {
                    id: "standard_producer".to_string(),
                    instance_count: 3,
                    topics: vec!["standard.events".to_string()],
                    target_rate: 20_000,
                    payload_config: PayloadConfig {
                        size_distribution: PayloadDistribution::Normal { mean: 2048.0, std_dev: 512.0 },
                        compression: Some(CompressionType::Lz4),
                        content_type: ContentType::Json,
                    },
                    load_pattern: LoadPattern::Sine {
                        min: 10_000,
                        max: 30_000,
                        period: Duration::from_minutes(15),
                    },
                    priority: 1,
                },
                // Low-frequency large messages
                ProducerConfig {
                    id: "bulk_producer".to_string(),
                    instance_count: 2,
                    topics: vec!["bulk.data".to_string()],
                    target_rate: 1_000,
                    payload_config: PayloadConfig {
                        size_distribution: PayloadDistribution::Uniform { min: 64 * 1024, max: 1024 * 1024 },
                        compression: Some(CompressionType::Zstd),
                        content_type: ContentType::Random,
                    },
                    load_pattern: LoadPattern::Step {
                        steps: vec![
                            (500, Duration::from_minutes(5)),
                            (1500, Duration::from_minutes(5)),
                            (1000, Duration::from_minutes(5)),
                        ],
                    },
                    priority: 3,
                },
            ],
            consumers: vec![
                // Fast micro-event processing
                ConsumerConfig {
                    id: "micro_consumer".to_string(),
                    instance_count: 8,
                    topics: vec!["micro.events".to_string()],
                    rate_limit: None,
                    batch_size: 1000,
                    processing_delay: Duration::from_micros(5),
                    consumer_group: "micro_processors".to_string(),
                },
                // Standard event processing
                ConsumerConfig {
                    id: "standard_consumer".to_string(),
                    instance_count: 4,
                    topics: vec!["standard.events".to_string()],
                    rate_limit: Some(25_000),
                    batch_size: 100,
                    processing_delay: Duration::from_micros(50),
                    consumer_group: "standard_processors".to_string(),
                },
                // Bulk data processing
                ConsumerConfig {
                    id: "bulk_consumer".to_string(),
                    instance_count: 2,
                    topics: vec!["bulk.data".to_string()],
                    rate_limit: Some(1_200),
                    batch_size: 10,
                    processing_delay: Duration::from_millis(100),
                    consumer_group: "bulk_processors".to_string(),
                },
            ],
            duration: Duration::from_minutes(45),
            success_criteria: SuccessCriteria {
                min_throughput: 620_000, // Sum of all producer rates * 0.8
                max_p99_latency: Duration::from_millis(2),
                max_error_rate: 0.002,
                max_memory_usage: 10 * 1024 * 1024 * 1024, // 10GB
                resource_limits: ResourceLimits {
                    max_cpu_utilization: 0.75,
                    max_memory_utilization: 0.65,
                    max_network_utilization: 15 * 1024 * 1024 * 1024, // 15GB/sec
                    max_disk_utilization: 0.5,
                },
                custom_criteria: {
                    let mut criteria = HashMap::new();
                    criteria.insert("fairness".to_string(), "no_starvation".to_string());
                    criteria.insert("priority_handling".to_string(), "correct".to_string());
                    criteria.insert("resource_isolation".to_string(), "maintained".to_string());
                    criteria
                },
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("test_type".to_string(), "mixed_workload".to_string());
                meta.insert("workload_types".to_string(), "micro_standard_bulk".to_string());
                meta
            },
        }
    }

    /// Convert to workload simulator configuration
    pub fn to_workload_simulator(&self) -> WorkloadSimulator {
        let mut simulator = WorkloadSimulator::new();

        // Add producers
        for producer_config in &self.producers {
            for instance in 0..producer_config.instance_count {
                let workload = ProducerWorkload {
                    id: Uuid::new_v4(),
                    topics: producer_config.topics.clone(),
                    rate: producer_config.load_pattern.clone(),
                    payload_size: producer_config.payload_config.size_distribution.clone(),
                    key_strategy: crate::generators::workload::KeyStrategy::Random,
                    compression: producer_config.payload_config.compression.clone(),
                    priority: producer_config.priority,
                    metadata: {
                        let mut meta = HashMap::new();
                        meta.insert("config_id".to_string(), producer_config.id.clone());
                        meta.insert("instance".to_string(), instance.to_string());
                        meta
                    },
                };
                simulator.add_producer(workload);
            }
        }

        // Add consumers
        for consumer_config in &self.consumers {
            for instance in 0..consumer_config.instance_count {
                let workload = ConsumerWorkload {
                    id: Uuid::new_v4(),
                    topics: consumer_config.topics.clone(),
                    consume_rate: consumer_config.rate_limit,
                    batch_size: consumer_config.batch_size,
                    processing_time: consumer_config.processing_delay,
                    consumer_group: Some(consumer_config.consumer_group.clone()),
                    auto_commit_interval: Duration::from_millis(100),
                    metadata: {
                        let mut meta = HashMap::new();
                        meta.insert("config_id".to_string(), consumer_config.id.clone());
                        meta.insert("instance".to_string(), instance.to_string());
                        meta
                    },
                };
                simulator.add_consumer(workload);
            }
        }

        simulator
    }

    /// Validate scenario configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.producers.is_empty() {
            return Err("Scenario must have at least one producer".to_string());
        }

        if self.consumers.is_empty() {
            return Err("Scenario must have at least one consumer".to_string());
        }

        if self.duration.is_zero() {
            return Err("Scenario duration must be greater than zero".to_string());
        }

        // Validate producer/consumer topic overlap
        let producer_topics: std::collections::HashSet<_> = self.producers.iter()
            .flat_map(|p| &p.topics)
            .collect();
        let consumer_topics: std::collections::HashSet<_> = self.consumers.iter()
            .flat_map(|c| &c.topics)
            .collect();

        if producer_topics.is_disjoint(&consumer_topics) {
            return Err("Producers and consumers must share at least one topic".to_string());
        }

        Ok(())
    }
}

/// Scenario result evaluation
#[derive(Debug, Clone)]
pub struct ScenarioResult {
    /// Scenario name
    pub scenario_name: String,
    /// Test duration
    pub duration: Duration,
    /// Actual throughput achieved
    pub throughput: u64,
    /// Latency percentiles
    pub latency_percentiles: LatencyPercentiles,
    /// Error rate
    pub error_rate: f64,
    /// Resource utilization
    pub resource_utilization: ResourceUtilization,
    /// Custom metrics
    pub custom_metrics: HashMap<String, String>,
    /// Success status
    pub success: bool,
}

/// Latency percentile measurements
#[derive(Debug, Clone)]
pub struct LatencyPercentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub p999: Duration,
}

/// Resource utilization measurements
#[derive(Debug, Clone)]
pub struct ResourceUtilization {
    pub avg_cpu_utilization: f64,
    pub peak_cpu_utilization: f64,
    pub avg_memory_utilization: f64,
    pub peak_memory_utilization: f64,
    pub network_throughput: u64,
    pub disk_utilization: f64,
}

impl ScenarioResult {
    /// Evaluate against success criteria
    pub fn evaluate_success(&self, criteria: &SuccessCriteria) -> bool {
        self.throughput >= criteria.min_throughput
            && self.latency_percentiles.p99 <= criteria.max_p99_latency
            && self.error_rate <= criteria.max_error_rate
            && self.resource_utilization.peak_cpu_utilization <= criteria.resource_limits.max_cpu_utilization
            && self.resource_utilization.peak_memory_utilization <= criteria.resource_limits.max_memory_utilization
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_throughput_scenario() {
        let scenario = LoadTestScenario::standard_throughput();
        assert_eq!(scenario.name, "standard_throughput");
        assert_eq!(scenario.producers.len(), 1);
        assert_eq!(scenario.consumers.len(), 1);
        assert!(scenario.validate().is_ok());
    }

    #[test]
    fn test_mixed_workload_scenario() {
        let scenario = LoadTestScenario::mixed_workload();
        assert_eq!(scenario.name, "mixed_workload");
        assert_eq!(scenario.producers.len(), 3); // micro, standard, bulk
        assert_eq!(scenario.consumers.len(), 3);
        assert!(scenario.validate().is_ok());
    }

    #[test]
    fn test_scenario_validation() {
        let mut scenario = LoadTestScenario::standard_throughput();
        
        // Valid scenario should pass
        assert!(scenario.validate().is_ok());
        
        // Remove all producers - should fail
        scenario.producers.clear();
        assert!(scenario.validate().is_err());
        
        // Add producer back but remove consumers - should fail
        scenario.producers = LoadTestScenario::standard_throughput().producers;
        scenario.consumers.clear();
        assert!(scenario.validate().is_err());
        
        // Add consumer back with different topics - should fail
        scenario.consumers = vec![ConsumerConfig {
            id: "different_consumer".to_string(),
            instance_count: 1,
            topics: vec!["different.topic".to_string()],
            rate_limit: None,
            batch_size: 100,
            processing_delay: Duration::from_micros(10),
            consumer_group: "different".to_string(),
        }];
        assert!(scenario.validate().is_err());
    }

    #[test]
    fn test_workload_simulator_conversion() {
        let scenario = LoadTestScenario::standard_throughput();
        let simulator = scenario.to_workload_simulator();
        
        // Should have created workloads for all producer and consumer instances
        let expected_producers = scenario.producers.iter()
            .map(|p| p.instance_count)
            .sum::<usize>();
        let expected_consumers = scenario.consumers.iter()
            .map(|c| c.instance_count)
            .sum::<usize>();
        
        // Note: We can't directly access the counts in WorkloadSimulator
        // In a real implementation, you'd add getters for this
    }

    #[test]
    fn test_success_criteria_evaluation() {
        let criteria = SuccessCriteria {
            min_throughput: 100_000,
            max_p99_latency: Duration::from_millis(1),
            max_error_rate: 0.01,
            max_memory_usage: 1024 * 1024 * 1024,
            resource_limits: ResourceLimits {
                max_cpu_utilization: 0.8,
                max_memory_utilization: 0.7,
                max_network_utilization: 1024 * 1024 * 1024,
                max_disk_utilization: 0.5,
            },
            custom_criteria: HashMap::new(),
        };

        let good_result = ScenarioResult {
            scenario_name: "test".to_string(),
            duration: Duration::from_minutes(10),
            throughput: 150_000,
            latency_percentiles: LatencyPercentiles {
                p50: Duration::from_micros(100),
                p90: Duration::from_micros(200),
                p95: Duration::from_micros(500),
                p99: Duration::from_micros(800),
                p999: Duration::from_micros(1500),
            },
            error_rate: 0.005,
            resource_utilization: ResourceUtilization {
                avg_cpu_utilization: 0.6,
                peak_cpu_utilization: 0.75,
                avg_memory_utilization: 0.5,
                peak_memory_utilization: 0.65,
                network_throughput: 500 * 1024 * 1024,
                disk_utilization: 0.3,
            },
            custom_metrics: HashMap::new(),
            success: true,
        };

        assert!(good_result.evaluate_success(&criteria));

        let bad_result = ScenarioResult {
            throughput: 50_000, // Below minimum
            latency_percentiles: LatencyPercentiles {
                p99: Duration::from_millis(2), // Above maximum
                ..good_result.latency_percentiles.clone()
            },
            ..good_result.clone()
        };

        assert!(!bad_result.evaluate_success(&criteria));
    }
}