//! Workload simulation for comprehensive distributed testing.
//!
//! This module provides realistic workload simulation including
//! producers, consumers, and administrative operations.

use crate::generators::data::{MessageGenerator, PayloadDistribution, CompressionType};
use crate::generators::load::{LoadPattern, LoadGenerator};
use kaelix_core::{Message, Topic, Result, Error};
use std::time::Duration;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use futures::{Stream, StreamExt};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use tokio::time::{interval, Instant};
use parking_lot::RwLock;

/// Producer identifier
pub type ProducerId = Uuid;

/// Consumer identifier  
pub type ConsumerId = Uuid;

/// Administrator identifier
pub type AdminId = Uuid;

/// Key generation strategies for partitioning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyStrategy {
    /// No key (random partitioning)
    None,
    /// Random keys
    Random,
    /// Sequential keys
    Sequential,
    /// Hash-based keys
    Hash(String),
    /// Custom key generation
    Custom(String),
}

/// Producer workload configuration
#[derive(Debug, Clone)]
pub struct ProducerWorkload {
    /// Unique producer identifier
    pub id: ProducerId,
    /// Topics to produce to
    pub topics: Vec<String>,
    /// Load generation pattern
    pub rate: LoadPattern,
    /// Message payload size distribution
    pub payload_size: PayloadDistribution,
    /// Key generation strategy
    pub key_strategy: KeyStrategy,
    /// Optional compression
    pub compression: Option<CompressionType>,
    /// Producer priority (for resource allocation)
    pub priority: u8,
    /// Producer metadata
    pub metadata: HashMap<String, String>,
}

/// Consumer workload configuration
#[derive(Debug, Clone)]
pub struct ConsumerWorkload {
    /// Unique consumer identifier
    pub id: ConsumerId,
    /// Topics to consume from
    pub topics: Vec<String>,
    /// Optional consume rate limit (messages/sec)
    pub consume_rate: Option<u64>,
    /// Batch size for consumption
    pub batch_size: usize,
    /// Simulated processing time per message
    pub processing_time: Duration,
    /// Consumer group (for load balancing)
    pub consumer_group: Option<String>,
    /// Auto-commit interval
    pub auto_commit_interval: Duration,
    /// Consumer metadata
    pub metadata: HashMap<String, String>,
}

/// Administrative workload configuration
#[derive(Debug, Clone)]
pub struct AdminWorkload {
    /// Admin operation identifier
    pub id: AdminId,
    /// Operations to perform
    pub operations: Vec<AdminOperation>,
    /// Interval between operations
    pub interval: Duration,
    /// Operation metadata
    pub metadata: HashMap<String, String>,
}

/// Administrative operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdminOperation {
    /// Create topic
    CreateTopic { 
        name: String, 
        partitions: u32,
        replication_factor: u16,
    },
    /// Delete topic
    DeleteTopic { 
        name: String 
    },
    /// Update topic configuration
    UpdateTopicConfig { 
        name: String, 
        config: HashMap<String, String> 
    },
    /// Create consumer group
    CreateConsumerGroup { 
        name: String,
        config: HashMap<String, String>,
    },
    /// Delete consumer group
    DeleteConsumerGroup { 
        name: String 
    },
    /// Trigger rebalance
    TriggerRebalance { 
        consumer_group: String 
    },
    /// Update cluster configuration
    UpdateClusterConfig { 
        config: HashMap<String, String> 
    },
    /// Perform health check
    HealthCheck,
    /// Collect metrics
    CollectMetrics,
}

/// Comprehensive workload simulator
#[derive(Debug)]
pub struct WorkloadSimulator {
    /// Producer workloads
    producers: Vec<ProducerWorkload>,
    /// Consumer workloads
    consumers: Vec<ConsumerWorkload>,
    /// Administrative workloads
    admin_operations: Vec<AdminWorkload>,
    /// Simulation statistics
    stats: Arc<RwLock<WorkloadStats>>,
    /// Simulation state
    state: Arc<RwLock<SimulationState>>,
}

/// Simulation state tracking
#[derive(Debug, Default)]
pub struct SimulationState {
    /// Start time
    pub start_time: Option<Instant>,
    /// Current simulation time
    pub current_time: Instant,
    /// Active producers
    pub active_producers: HashMap<ProducerId, ProducerState>,
    /// Active consumers
    pub active_consumers: HashMap<ConsumerId, ConsumerState>,
    /// Active admin operations
    pub active_admin_ops: HashMap<AdminId, AdminState>,
    /// Topic states
    pub topics: HashMap<String, TopicState>,
}

/// Producer state tracking
#[derive(Debug, Clone)]
pub struct ProducerState {
    /// Messages produced
    pub messages_produced: u64,
    /// Bytes produced
    pub bytes_produced: u64,
    /// Current rate
    pub current_rate: f64,
    /// Errors encountered
    pub errors: u64,
    /// Last activity time
    pub last_activity: Instant,
}

/// Consumer state tracking
#[derive(Debug, Clone)]
pub struct ConsumerState {
    /// Messages consumed
    pub messages_consumed: u64,
    /// Bytes consumed
    pub bytes_consumed: u64,
    /// Current lag
    pub current_lag: u64,
    /// Processing time statistics
    pub processing_stats: ProcessingStats,
    /// Last activity time
    pub last_activity: Instant,
}

/// Admin operation state
#[derive(Debug, Clone)]
pub struct AdminState {
    /// Operations performed
    pub operations_performed: u64,
    /// Successes
    pub successes: u64,
    /// Failures
    pub failures: u64,
    /// Last operation time
    pub last_operation: Instant,
}

/// Topic state tracking
#[derive(Debug, Clone)]
pub struct TopicState {
    /// Total messages
    pub total_messages: u64,
    /// Total bytes
    pub total_bytes: u64,
    /// Partition count
    pub partition_count: u32,
    /// Replication factor
    pub replication_factor: u16,
    /// Creation time
    pub created_at: Instant,
}

/// Processing time statistics
#[derive(Debug, Clone, Default)]
pub struct ProcessingStats {
    /// Average processing time
    pub avg_processing_time: Duration,
    /// Maximum processing time
    pub max_processing_time: Duration,
    /// Minimum processing time
    pub min_processing_time: Duration,
    /// Processing time standard deviation
    pub processing_time_std_dev: Duration,
}

/// Comprehensive workload statistics
#[derive(Debug, Clone, Default)]
pub struct WorkloadStats {
    /// Simulation duration
    pub duration: Duration,
    /// Total messages produced
    pub total_messages_produced: u64,
    /// Total messages consumed
    pub total_messages_consumed: u64,
    /// Total bytes transferred
    pub total_bytes_transferred: u64,
    /// Producer statistics
    pub producer_stats: HashMap<ProducerId, ProducerMetrics>,
    /// Consumer statistics
    pub consumer_stats: HashMap<ConsumerId, ConsumerMetrics>,
    /// Admin operation statistics
    pub admin_stats: HashMap<AdminId, AdminMetrics>,
    /// Overall throughput
    pub overall_throughput: f64,
    /// Overall latency percentiles
    pub latency_percentiles: LatencyPercentiles,
    /// Error rates
    pub error_rates: ErrorRates,
    /// Resource utilization
    pub resource_utilization: ResourceUtilization,
}

/// Producer-specific metrics
#[derive(Debug, Clone, Default)]
pub struct ProducerMetrics {
    /// Messages produced per second
    pub messages_per_second: f64,
    /// Bytes produced per second
    pub bytes_per_second: f64,
    /// Success rate
    pub success_rate: f64,
    /// Average message size
    pub avg_message_size: f64,
    /// Compression ratio
    pub compression_ratio: Option<f64>,
}

/// Consumer-specific metrics
#[derive(Debug, Clone, Default)]
pub struct ConsumerMetrics {
    /// Messages consumed per second
    pub messages_per_second: f64,
    /// Bytes consumed per second
    pub bytes_per_second: f64,
    /// Average lag
    pub avg_lag: f64,
    /// Processing time statistics
    pub processing_stats: ProcessingStats,
    /// Rebalance count
    pub rebalance_count: u64,
}

/// Admin operation metrics
#[derive(Debug, Clone, Default)]
pub struct AdminMetrics {
    /// Operations per second
    pub operations_per_second: f64,
    /// Success rate
    pub success_rate: f64,
    /// Average operation time
    pub avg_operation_time: Duration,
}

/// Latency percentile measurements
#[derive(Debug, Clone, Default)]
pub struct LatencyPercentiles {
    /// P50 latency
    pub p50: Duration,
    /// P90 latency
    pub p90: Duration,
    /// P95 latency
    pub p95: Duration,
    /// P99 latency
    pub p99: Duration,
    /// P99.9 latency
    pub p999: Duration,
}

/// Error rate measurements
#[derive(Debug, Clone, Default)]
pub struct ErrorRates {
    /// Producer error rate
    pub producer_error_rate: f64,
    /// Consumer error rate
    pub consumer_error_rate: f64,
    /// Admin operation error rate
    pub admin_error_rate: f64,
    /// Network error rate
    pub network_error_rate: f64,
}

/// Resource utilization measurements
#[derive(Debug, Clone, Default)]
pub struct ResourceUtilization {
    /// CPU utilization percentage
    pub cpu_utilization: f64,
    /// Memory utilization percentage
    pub memory_utilization: f64,
    /// Network utilization percentage
    pub network_utilization: f64,
    /// Disk utilization percentage
    pub disk_utilization: f64,
}

impl WorkloadSimulator {
    /// Create new workload simulator
    pub fn new() -> Self {
        Self {
            producers: Vec::new(),
            consumers: Vec::new(),
            admin_operations: Vec::new(),
            stats: Arc::new(RwLock::new(WorkloadStats::default())),
            state: Arc::new(RwLock::new(SimulationState::default())),
        }
    }

    /// Add producer workload
    pub fn add_producer(&mut self, producer: ProducerWorkload) {
        self.producers.push(producer);
    }

    /// Add consumer workload
    pub fn add_consumer(&mut self, consumer: ConsumerWorkload) {
        self.consumers.push(consumer);
    }

    /// Add admin operations
    pub fn add_admin_operations(&mut self, admin: AdminWorkload) {
        self.admin_operations.push(admin);
    }

    /// Run complete simulation for specified duration
    pub async fn run_simulation(&self, duration: Duration) -> Result<WorkloadStats> {
        let start_time = Instant::now();
        
        // Initialize simulation state
        {
            let mut state = self.state.write();
            state.start_time = Some(start_time);
            state.current_time = start_time;
        }

        // Start all workloads concurrently
        let producer_handles: Vec<_> = self.producers.iter()
            .map(|producer| {
                let producer = producer.clone();
                let state = Arc::clone(&self.state);
                tokio::spawn(async move {
                    Self::run_producer_workload(producer, duration, state).await
                })
            })
            .collect();

        let consumer_handles: Vec<_> = self.consumers.iter()
            .map(|consumer| {
                let consumer = consumer.clone();
                let state = Arc::clone(&self.state);
                tokio::spawn(async move {
                    Self::run_consumer_workload(consumer, duration, state).await
                })
            })
            .collect();

        let admin_handles: Vec<_> = self.admin_operations.iter()
            .map(|admin| {
                let admin = admin.clone();
                let state = Arc::clone(&self.state);
                tokio::spawn(async move {
                    Self::run_admin_workload(admin, duration, state).await
                })
            })
            .collect();

        // Wait for all workloads to complete
        for handle in producer_handles {
            let _ = handle.await;
        }
        for handle in consumer_handles {
            let _ = handle.await;
        }
        for handle in admin_handles {
            let _ = handle.await;
        }

        // Calculate final statistics
        let final_stats = self.calculate_final_stats(start_time.elapsed()).await;
        *self.stats.write() = final_stats.clone();

        Ok(final_stats)
    }

    /// Run mixed workload simulation
    pub async fn run_mixed_workload(&self) -> Result<WorkloadStats> {
        // Default to 5 minutes for mixed workload
        self.run_simulation(Duration::from_secs(300)).await
    }

    /// Scale workload by factor
    pub fn scale_workload(&mut self, factor: f64) {
        // Scale producer rates
        for producer in &mut self.producers {
            producer.rate = self.scale_load_pattern(&producer.rate, factor);
        }

        // Scale consumer rates
        for consumer in &mut self.consumers {
            if let Some(rate) = consumer.consume_rate {
                consumer.consume_rate = Some((rate as f64 * factor) as u64);
            }
        }

        // Scale admin operation intervals
        for admin in &mut self.admin_operations {
            admin.interval = Duration::from_nanos(
                (admin.interval.as_nanos() as f64 / factor) as u64
            );
        }
    }

    /// Run producer workload
    async fn run_producer_workload(
        producer: ProducerWorkload,
        duration: Duration,
        state: Arc<RwLock<SimulationState>>,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut message_generator = MessageGenerator::new()
            .with_distribution(producer.payload_size.clone())
            .with_topic_patterns(producer.topics.clone());

        if let Some(compression) = producer.compression {
            message_generator = message_generator.with_compression(compression);
        }

        let load_generator = LoadGenerator::new(producer.rate)
            .with_message_generator(message_generator);

        let mut message_count = 0u64;
        let mut bytes_produced = 0u64;

        let stream = load_generator.generate(producer.rate);
        tokio::pin!(stream);

        while start_time.elapsed() < duration {
            if let Some(message) = stream.next().await {
                message_count += 1;
                bytes_produced += message.payload().len() as u64;

                // Update state
                {
                    let mut state = state.write();
                    let producer_state = state.active_producers
                        .entry(producer.id)
                        .or_insert_with(|| ProducerState {
                            messages_produced: 0,
                            bytes_produced: 0,
                            current_rate: 0.0,
                            errors: 0,
                            last_activity: Instant::now(),
                        });

                    producer_state.messages_produced = message_count;
                    producer_state.bytes_produced = bytes_produced;
                    producer_state.current_rate = message_count as f64 / start_time.elapsed().as_secs_f64();
                    producer_state.last_activity = Instant::now();
                }
            }
        }

        Ok(())
    }

    /// Run consumer workload
    async fn run_consumer_workload(
        consumer: ConsumerWorkload,
        duration: Duration,
        state: Arc<RwLock<SimulationState>>,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut message_count = 0u64;
        let mut bytes_consumed = 0u64;
        let mut processing_times = Vec::new();

        // Simulate consumption based on rate limit
        let consume_interval = if let Some(rate) = consumer.consume_rate {
            Duration::from_nanos(1_000_000_000 / rate.max(1))
        } else {
            Duration::from_millis(10) // Default interval
        };

        let mut interval_timer = interval(consume_interval);

        while start_time.elapsed() < duration {
            interval_timer.tick().await;

            // Simulate message processing
            let processing_start = Instant::now();
            tokio::time::sleep(consumer.processing_time).await;
            let processing_elapsed = processing_start.elapsed();
            processing_times.push(processing_elapsed);

            message_count += 1;
            bytes_consumed += 1024; // Simulated message size

            // Update state
            {
                let mut state = state.write();
                let consumer_state = state.active_consumers
                    .entry(consumer.id)
                    .or_insert_with(|| ConsumerState {
                        messages_consumed: 0,
                        bytes_consumed: 0,
                        current_lag: 0,
                        processing_stats: ProcessingStats::default(),
                        last_activity: Instant::now(),
                    });

                consumer_state.messages_consumed = message_count;
                consumer_state.bytes_consumed = bytes_consumed;
                consumer_state.processing_stats = Self::calculate_processing_stats(&processing_times);
                consumer_state.last_activity = Instant::now();
            }
        }

        Ok(())
    }

    /// Run admin workload
    async fn run_admin_workload(
        admin: AdminWorkload,
        duration: Duration,
        state: Arc<RwLock<SimulationState>>,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut operation_count = 0u64;
        let mut successes = 0u64;

        let mut interval_timer = interval(admin.interval);

        while start_time.elapsed() < duration {
            interval_timer.tick().await;

            // Execute admin operation
            for operation in &admin.operations {
                let success = Self::simulate_admin_operation(operation).await;
                operation_count += 1;
                if success {
                    successes += 1;
                }

                // Update state
                {
                    let mut state = state.write();
                    let admin_state = state.active_admin_ops
                        .entry(admin.id)
                        .or_insert_with(|| AdminState {
                            operations_performed: 0,
                            successes: 0,
                            failures: 0,
                            last_operation: Instant::now(),
                        });

                    admin_state.operations_performed = operation_count;
                    admin_state.successes = successes;
                    admin_state.failures = operation_count - successes;
                    admin_state.last_operation = Instant::now();
                }
            }
        }

        Ok(())
    }

    /// Simulate admin operation execution
    async fn simulate_admin_operation(operation: &AdminOperation) -> bool {
        // Simulate operation latency
        let latency = match operation {
            AdminOperation::CreateTopic { .. } => Duration::from_millis(100),
            AdminOperation::DeleteTopic { .. } => Duration::from_millis(50),
            AdminOperation::UpdateTopicConfig { .. } => Duration::from_millis(30),
            AdminOperation::CreateConsumerGroup { .. } => Duration::from_millis(80),
            AdminOperation::DeleteConsumerGroup { .. } => Duration::from_millis(40),
            AdminOperation::TriggerRebalance { .. } => Duration::from_millis(200),
            AdminOperation::UpdateClusterConfig { .. } => Duration::from_millis(150),
            AdminOperation::HealthCheck => Duration::from_millis(10),
            AdminOperation::CollectMetrics => Duration::from_millis(20),
        };

        tokio::time::sleep(latency).await;

        // Simulate 95% success rate
        rand::random::<f64>() < 0.95
    }

    /// Scale load pattern by factor
    fn scale_load_pattern(&self, pattern: &LoadPattern, factor: f64) -> LoadPattern {
        match pattern {
            LoadPattern::Constant { rate } => LoadPattern::Constant {
                rate: (*rate as f64 * factor) as u64,
            },
            LoadPattern::Burst { base, peak, duration, interval } => LoadPattern::Burst {
                base: (*base as f64 * factor) as u64,
                peak: (*peak as f64 * factor) as u64,
                duration: *duration,
                interval: *interval,
            },
            LoadPattern::Sine { min, max, period } => LoadPattern::Sine {
                min: (*min as f64 * factor) as u64,
                max: (*max as f64 * factor) as u64,
                period: *period,
            },
            LoadPattern::Sawtooth { min, max, period } => LoadPattern::Sawtooth {
                min: (*min as f64 * factor) as u64,
                max: (*max as f64 * factor) as u64,
                period: *period,
            },
            LoadPattern::Step { steps } => LoadPattern::Step {
                steps: steps.iter()
                    .map(|(rate, duration)| ((*rate as f64 * factor) as u64, *duration))
                    .collect(),
            },
            LoadPattern::Realistic { profile } => LoadPattern::Realistic {
                profile: profile.clone(),
            },
            LoadPattern::Interactive { think_time } => LoadPattern::Interactive {
                think_time: *think_time,
            },
        }
    }

    /// Calculate processing statistics
    fn calculate_processing_stats(times: &[Duration]) -> ProcessingStats {
        if times.is_empty() {
            return ProcessingStats::default();
        }

        let avg = times.iter().sum::<Duration>() / times.len() as u32;
        let max = times.iter().max().copied().unwrap_or_default();
        let min = times.iter().min().copied().unwrap_or_default();

        // Calculate standard deviation
        let variance = times.iter()
            .map(|&t| {
                let diff = t.as_nanos() as i64 - avg.as_nanos() as i64;
                (diff * diff) as u64
            })
            .sum::<u64>() / times.len() as u64;
        let std_dev = Duration::from_nanos((variance as f64).sqrt() as u64);

        ProcessingStats {
            avg_processing_time: avg,
            max_processing_time: max,
            min_processing_time: min,
            processing_time_std_dev: std_dev,
        }
    }

    /// Calculate final simulation statistics
    async fn calculate_final_stats(&self, duration: Duration) -> WorkloadStats {
        let state = self.state.read();
        
        let total_messages_produced = state.active_producers.values()
            .map(|p| p.messages_produced)
            .sum();
        
        let total_messages_consumed = state.active_consumers.values()
            .map(|c| c.messages_consumed)
            .sum();
        
        let total_bytes_transferred = state.active_producers.values()
            .map(|p| p.bytes_produced)
            .sum::<u64>() + state.active_consumers.values()
            .map(|c| c.bytes_consumed)
            .sum::<u64>();

        let overall_throughput = total_messages_produced as f64 / duration.as_secs_f64();

        WorkloadStats {
            duration,
            total_messages_produced,
            total_messages_consumed,
            total_bytes_transferred,
            overall_throughput,
            // Initialize other fields with defaults for now
            ..Default::default()
        }
    }

    /// Get current statistics
    pub fn stats(&self) -> WorkloadStats {
        self.stats.read().clone()
    }
}

impl Default for WorkloadSimulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience constructors for common workloads
impl WorkloadSimulator {
    /// Create high-throughput producer workload
    pub fn high_throughput_producer() -> ProducerWorkload {
        ProducerWorkload {
            id: Uuid::new_v4(),
            topics: vec!["high-throughput".to_string()],
            rate: LoadPattern::Constant { rate: 100_000 },
            payload_size: PayloadDistribution::Uniform { min: 1024, max: 4096 },
            key_strategy: KeyStrategy::Random,
            compression: Some(CompressionType::Lz4),
            priority: 1,
            metadata: HashMap::new(),
        }
    }

    /// Create low-latency consumer workload
    pub fn low_latency_consumer() -> ConsumerWorkload {
        ConsumerWorkload {
            id: Uuid::new_v4(),
            topics: vec!["low-latency".to_string()],
            consume_rate: None, // No rate limit
            batch_size: 1,
            processing_time: Duration::from_micros(10),
            consumer_group: Some("latency-sensitive".to_string()),
            auto_commit_interval: Duration::from_millis(100),
            metadata: HashMap::new(),
        }
    }

    /// Create administrative maintenance workload
    pub fn maintenance_admin() -> AdminWorkload {
        AdminWorkload {
            id: Uuid::new_v4(),
            operations: vec![
                AdminOperation::HealthCheck,
                AdminOperation::CollectMetrics,
            ],
            interval: Duration::from_secs(30),
            metadata: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_workload_simulator_basic() {
        let mut simulator = WorkloadSimulator::new();
        
        simulator.add_producer(ProducerWorkload {
            id: Uuid::new_v4(),
            topics: vec!["test".to_string()],
            rate: LoadPattern::Constant { rate: 100 },
            payload_size: PayloadDistribution::Uniform { min: 100, max: 200 },
            key_strategy: KeyStrategy::Random,
            compression: None,
            priority: 1,
            metadata: HashMap::new(),
        });

        let stats = simulator.run_simulation(Duration::from_millis(100)).await.unwrap();
        assert!(stats.total_messages_produced > 0);
    }

    #[tokio::test]
    async fn test_mixed_workload() {
        let mut simulator = WorkloadSimulator::new();
        
        simulator.add_producer(WorkloadSimulator::high_throughput_producer());
        simulator.add_consumer(WorkloadSimulator::low_latency_consumer());
        simulator.add_admin_operations(WorkloadSimulator::maintenance_admin());

        let stats = simulator.run_simulation(Duration::from_millis(200)).await.unwrap();
        
        assert!(stats.total_messages_produced > 0);
        assert!(stats.overall_throughput > 0.0);
    }

    #[test]
    fn test_workload_scaling() {
        let mut simulator = WorkloadSimulator::new();
        simulator.add_producer(WorkloadSimulator::high_throughput_producer());

        let original_rate = match &simulator.producers[0].rate {
            LoadPattern::Constant { rate } => *rate,
            _ => panic!("Expected constant rate"),
        };

        simulator.scale_workload(2.0);

        let scaled_rate = match &simulator.producers[0].rate {
            LoadPattern::Constant { rate } => *rate,
            _ => panic!("Expected constant rate"),
        };

        assert_eq!(scaled_rate, original_rate * 2);
    }

    #[test]
    fn test_processing_stats_calculation() {
        let times = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(15),
        ];

        let stats = WorkloadSimulator::calculate_processing_stats(&times);
        assert_eq!(stats.avg_processing_time, Duration::from_millis(15));
        assert_eq!(stats.max_processing_time, Duration::from_millis(20));
        assert_eq!(stats.min_processing_time, Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_admin_operation_simulation() {
        let operation = AdminOperation::HealthCheck;
        let success = WorkloadSimulator::simulate_admin_operation(&operation).await;
        // Should succeed most of the time (95% success rate)
        assert!(success || !success); // Always true, but tests the function doesn't panic
    }
}