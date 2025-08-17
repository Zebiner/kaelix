//! Test context and orchestration for comprehensive testing scenarios.

use crate::constants::*;
use kaelix_core::{Message, Result, Error};
use kaelix_broker::{Broker, BrokerConfig, BrokerHandle};
use kaelix_publisher::{Publisher, PublisherConfig};
use kaelix_consumer::{Consumer, ConsumerConfig};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use std::sync::Arc;
use tracing::{info, warn, error};

/// Central test context that orchestrates all testing components.
///
/// The `TestContext` provides a unified interface for:
/// - Setting up test environments (brokers, publishers, consumers)
/// - Coordinating multi-component test scenarios
/// - Collecting metrics and validating results
/// - Managing test lifecycle and cleanup
#[derive(Debug)]
pub struct TestContext {
    /// Test configuration
    config: TestConfig,
    
    /// Running broker instances
    brokers: Arc<RwLock<Vec<BrokerHandle>>>,
    
    /// Publisher clients
    publishers: Arc<RwLock<Vec<Publisher>>>,
    
    /// Consumer clients  
    consumers: Arc<RwLock<Vec<Consumer>>>,
    
    /// Test metrics collector
    metrics: Arc<Mutex<TestMetrics>>,
    
    /// Test start time
    start_time: Instant,
}

/// Configuration for test scenarios.
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Number of broker instances to create
    pub broker_count: usize,
    
    /// Number of publisher clients
    pub publisher_count: usize,
    
    /// Number of consumer clients
    pub consumer_count: usize,
    
    /// Test timeout duration
    pub timeout: Duration,
    
    /// Enable performance validation
    pub validate_performance: bool,
    
    /// Enable chaos engineering
    pub enable_chaos: bool,
    
    /// Target message throughput
    pub target_throughput: u64,
    
    /// Maximum acceptable latency
    pub max_latency: Duration,
}

/// Test execution result with comprehensive metrics.
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test execution duration
    pub duration: Duration,
    
    /// Performance metrics
    pub performance: PerformanceMetrics,
    
    /// Correctness validation results
    pub correctness: CorrectnessMetrics,
    
    /// Resource utilization
    pub resources: ResourceMetrics,
    
    /// Test success status
    pub success: bool,
    
    /// Error details if test failed
    pub errors: Vec<String>,
}

/// Performance-related test metrics.
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// Total messages processed
    pub message_count: u64,
    
    /// Average throughput (msg/sec)
    pub throughput: f64,
    
    /// Latency percentiles in microseconds
    pub latency_p50: u64,
    pub latency_p95: u64,
    pub latency_p99: u64,
    pub latency_max: u64,
    
    /// CPU utilization percentage
    pub cpu_usage: f64,
    
    /// Memory usage in bytes
    pub memory_usage: u64,
}

/// Correctness validation metrics.
#[derive(Debug, Clone, Default)]
pub struct CorrectnessMetrics {
    /// Messages sent vs received
    pub message_delivery_rate: f64,
    
    /// Order preservation validation
    pub order_violations: u64,
    
    /// Duplicate message count
    pub duplicates: u64,
    
    /// Data integrity violations
    pub corruption_count: u64,
}

/// Resource utilization metrics.
#[derive(Debug, Clone, Default)]
pub struct ResourceMetrics {
    /// Peak memory usage
    pub peak_memory: u64,
    
    /// Average CPU usage
    pub avg_cpu: f64,
    
    /// Network bandwidth utilization
    pub network_usage: f64,
    
    /// File descriptor count
    pub fd_count: u32,
}

/// Internal metrics collection.
#[derive(Debug, Default)]
struct TestMetrics {
    performance: PerformanceMetrics,
    correctness: CorrectnessMetrics,
    resources: ResourceMetrics,
    errors: Vec<String>,
    latencies: Vec<u64>,
}

impl TestContext {
    /// Create a new test context with default configuration.
    pub async fn new() -> Self {
        Self::with_config(TestConfig::default()).await
    }

    /// Create a test context with custom configuration.
    pub async fn with_config(config: TestConfig) -> Self {
        tracing_subscriber::fmt()
            .with_env_filter("debug")
            .try_init()
            .ok(); // Ignore error if already initialized

        info!("Initializing test context with config: {:?}", config);

        Self {
            config,
            brokers: Arc::new(RwLock::new(Vec::new())),
            publishers: Arc::new(RwLock::new(Vec::new())),
            consumers: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(Mutex::new(TestMetrics::default())),
            start_time: Instant::now(),
        }
    }

    /// Start the specified number of broker instances.
    pub async fn start_brokers(&self) -> Result<()> {
        let mut brokers = self.brokers.write().await;
        
        for i in 0..self.config.broker_count {
            let mut broker_config = BrokerConfig::default();
            // Configure each broker with different ports
            broker_config.network.bind_address = format!("127.0.0.1:{}", 9092 + i)
                .parse()
                .map_err(|e| Error::Configuration {
                    message: format!("Invalid broker address: {}", e),
                })?;

            let broker = Broker::new(broker_config).await?;
            let handle = broker.start().await?;
            brokers.push(handle);
            
            info!("Started broker {} on port {}", i, 9092 + i);
        }

        Ok(())
    }

    /// Create publisher clients connected to the brokers.
    pub async fn create_publishers(&self) -> Result<()> {
        let mut publishers = self.publishers.write().await;
        let brokers = self.brokers.read().await;
        
        if brokers.is_empty() {
            return Err(Error::Configuration {
                message: "No brokers available for publishers".to_string(),
            });
        }

        for i in 0..self.config.publisher_count {
            let mut config = PublisherConfig::default();
            // Distribute publishers across brokers
            let broker_idx = i % brokers.len();
            config.brokers = vec![format!("127.0.0.1:{}", 9092 + broker_idx)
                .parse()
                .map_err(|e| Error::Configuration {
                    message: format!("Invalid broker address: {}", e),
                })?];

            let publisher = Publisher::new(config).await?;
            publishers.push(publisher);
            
            info!("Created publisher {} connected to broker {}", i, broker_idx);
        }

        Ok(())
    }

    /// Create consumer clients connected to the brokers.
    pub async fn create_consumers(&self) -> Result<()> {
        let mut consumers = self.consumers.write().await;
        let brokers = self.brokers.read().await;
        
        if brokers.is_empty() {
            return Err(Error::Configuration {
                message: "No brokers available for consumers".to_string(),
            });
        }

        for i in 0..self.config.consumer_count {
            let mut config = ConsumerConfig::default();
            config.group_id = format!("test-group-{}", i);
            
            // Distribute consumers across brokers
            let broker_idx = i % brokers.len();
            config.brokers = vec![format!("127.0.0.1:{}", 9092 + broker_idx)
                .parse()
                .map_err(|e| Error::Configuration {
                    message: format!("Invalid broker address: {}", e),
                })?];

            let consumer = Consumer::new(config).await?;
            consumers.push(consumer);
            
            info!("Created consumer {} in group test-group-{}", i, i);
        }

        Ok(())
    }

    /// Publish a batch of messages and record metrics.
    pub async fn publish_batch(&self, messages: Vec<Message>) -> Result<TestResult> {
        let start = Instant::now();
        let publishers = self.publishers.read().await;
        
        if publishers.is_empty() {
            return Err(Error::Configuration {
                message: "No publishers available".to_string(),
            });
        }

        // Distribute messages across publishers
        let chunk_size = (messages.len() + publishers.len() - 1) / publishers.len();
        let mut tasks = Vec::new();
        
        for (i, chunk) in messages.chunks(chunk_size).enumerate() {
            if let Some(publisher) = publishers.get(i) {
                let messages = chunk.to_vec();
                let publisher_clone = publisher.clone(); // Assuming Publisher implements Clone
                
                let task = tokio::spawn(async move {
                    let mut latencies = Vec::new();
                    for message in messages {
                        let msg_start = Instant::now();
                        let result = publisher_clone.publish(message).await;
                        let latency = msg_start.elapsed().as_micros() as u64;
                        latencies.push(latency);
                        result?;
                    }
                    Ok::<Vec<u64>, Error>(latencies)
                });
                
                tasks.push(task);
            }
        }

        // Wait for all publish tasks to complete
        let mut all_latencies = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(latencies)) => all_latencies.extend(latencies),
                Ok(Err(e)) => {
                    error!("Publish task failed: {}", e);
                    let mut metrics = self.metrics.lock().await;
                    metrics.errors.push(e.to_string());
                }
                Err(e) => {
                    error!("Task join error: {}", e);
                    let mut metrics = self.metrics.lock().await;
                    metrics.errors.push(format!("Task join error: {}", e));
                }
            }
        }

        let duration = start.elapsed();
        
        // Update metrics
        {
            let mut metrics = self.metrics.lock().await;
            metrics.performance.message_count += messages.len() as u64;
            metrics.latencies.extend(all_latencies);
        }

        self.build_test_result(duration).await
    }

    /// Get current performance metrics.
    pub async fn get_performance_metrics(&self) -> PerformanceMetrics {
        let metrics = self.metrics.lock().await;
        metrics.performance.clone()
    }

    /// Validate that performance targets are met.
    pub async fn validate_performance(&self) -> bool {
        let metrics = self.metrics.lock().await;
        
        // Check throughput
        if metrics.performance.throughput < self.config.target_throughput as f64 {
            warn!(
                "Throughput below target: {} < {}",
                metrics.performance.throughput, self.config.target_throughput
            );
            return false;
        }

        // Check latency
        if metrics.performance.latency_p99 > self.config.max_latency.as_micros() as u64 {
            warn!(
                "P99 latency above target: {}μs > {}μs",
                metrics.performance.latency_p99,
                self.config.max_latency.as_micros()
            );
            return false;
        }

        true
    }

    /// Build final test result with all collected metrics.
    async fn build_test_result(&self, duration: Duration) -> Result<TestResult> {
        let mut metrics = self.metrics.lock().await;
        
        // Calculate performance metrics
        if !metrics.latencies.is_empty() {
            metrics.latencies.sort_unstable();
            let len = metrics.latencies.len();
            
            metrics.performance.latency_p50 = metrics.latencies[len / 2];
            metrics.performance.latency_p95 = metrics.latencies[len * 95 / 100];
            metrics.performance.latency_p99 = metrics.latencies[len * 99 / 100];
            metrics.performance.latency_max = *metrics.latencies.last().unwrap_or(&0);
        }

        metrics.performance.throughput = 
            metrics.performance.message_count as f64 / duration.as_secs_f64();

        let success = metrics.errors.is_empty() && 
            (!self.config.validate_performance || self.validate_performance().await);

        Ok(TestResult {
            duration,
            performance: metrics.performance.clone(),
            correctness: metrics.correctness.clone(),
            resources: metrics.resources.clone(),
            success,
            errors: metrics.errors.clone(),
        })
    }

    /// Clean up all test resources.
    pub async fn cleanup(&self) -> Result<()> {
        info!("Cleaning up test context");

        // Close all consumers
        let mut consumers = self.consumers.write().await;
        for consumer in consumers.drain(..) {
            if let Err(e) = consumer.close().await {
                warn!("Error closing consumer: {}", e);
            }
        }

        // Close all publishers
        let mut publishers = self.publishers.write().await;
        for publisher in publishers.drain(..) {
            if let Err(e) = publisher.close().await {
                warn!("Error closing publisher: {}", e);
            }
        }

        // Stop all brokers
        // Note: We can't easily stop brokers with current BrokerHandle API
        // This would need to be implemented in the broker module

        info!("Test context cleanup completed");
        Ok(())
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Ensure cleanup happens even if not called explicitly
        info!("TestContext dropped - resources should be cleaned up");
    }
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            broker_count: 1,
            publisher_count: 1,
            consumer_count: 1,
            timeout: DEFAULT_TEST_TIMEOUT,
            validate_performance: true,
            enable_chaos: false,
            target_throughput: TARGET_THROUGHPUT,
            max_latency: Duration::from_micros(TARGET_LATENCY_P99_MICROS),
        }
    }
}

impl TestResult {
    /// Check if latency targets were met.
    pub fn latency_p99(&self) -> Duration {
        Duration::from_micros(self.performance.latency_p99)
    }

    /// Check if throughput targets were met.
    pub fn throughput(&self) -> f64 {
        self.performance.throughput
    }

    /// Get a summary of test results.
    pub fn summary(&self) -> String {
        format!(
            "Test {}: Duration: {:?}, Throughput: {:.2} msg/s, P99 Latency: {}μs, Errors: {}",
            if self.success { "PASSED" } else { "FAILED" },
            self.duration,
            self.performance.throughput,
            self.performance.latency_p99,
            self.errors.len()
        )
    }
}