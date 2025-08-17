//! Benchmark setup utilities and common configurations.

use kaelix_core::{Message, Result};
use kaelix_tests::{TestContext, TestConfig, generators::MessageGenerator};
use bytes::Bytes;
use std::time::Duration;

/// Standard benchmark configurations.
pub struct BenchmarkConfig;

impl BenchmarkConfig {
    /// Configuration for throughput benchmarks.
    pub fn throughput() -> TestConfig {
        TestConfig {
            broker_count: 1,
            publisher_count: 1,
            consumer_count: 1,
            timeout: Duration::from_secs(60),
            validate_performance: false,
            enable_chaos: false,
            target_throughput: 10_000_000, // 10M msg/sec
            max_latency: Duration::from_micros(10),
        }
    }

    /// Configuration for latency benchmarks.
    pub fn latency() -> TestConfig {
        TestConfig {
            broker_count: 1,
            publisher_count: 1,
            consumer_count: 1,
            timeout: Duration::from_secs(30),
            validate_performance: false,
            enable_chaos: false,
            target_throughput: 100_000, // Lower throughput for latency focus
            max_latency: Duration::from_micros(5),
        }
    }

    /// Configuration for memory usage benchmarks.
    pub fn memory() -> TestConfig {
        TestConfig {
            broker_count: 1,
            publisher_count: 1,
            consumer_count: 1,
            timeout: Duration::from_secs(30),
            validate_performance: false,
            enable_chaos: false,
            target_throughput: 1_000_000,
            max_latency: Duration::from_micros(20),
        }
    }

    /// Configuration for concurrent operation benchmarks.
    pub fn concurrent() -> TestConfig {
        TestConfig {
            broker_count: 3,
            publisher_count: 10,
            consumer_count: 10,
            timeout: Duration::from_secs(60),
            validate_performance: false,
            enable_chaos: false,
            target_throughput: 5_000_000,
            max_latency: Duration::from_micros(15),
        }
    }
}

/// Test data generators for benchmarks.
pub struct BenchmarkData;

impl BenchmarkData {
    /// Generate small messages (64 bytes) for low-latency testing.
    pub fn small_messages(count: usize) -> Result<Vec<Message>> {
        let mut generator = MessageGenerator::new()
            .with_payload_size(64)
            .with_topics(vec!["bench-small".to_string()]);

        tokio::runtime::Runtime::new()
            .expect("Failed to create runtime")
            .block_on(generator.generate_batch(count))
    }

    /// Generate medium messages (1KB) for typical workloads.
    pub fn medium_messages(count: usize) -> Result<Vec<Message>> {
        let mut generator = MessageGenerator::new()
            .with_payload_size(1024)
            .with_topics(vec!["bench-medium".to_string()]);

        tokio::runtime::Runtime::new()
            .expect("Failed to create runtime")
            .block_on(generator.generate_batch(count))
    }

    /// Generate large messages (64KB) for throughput testing.
    pub fn large_messages(count: usize) -> Result<Vec<Message>> {
        let mut generator = MessageGenerator::new()
            .with_payload_size(65536)
            .with_topics(vec!["bench-large".to_string()]);

        tokio::runtime::Runtime::new()
            .expect("Failed to create runtime")
            .block_on(generator.generate_batch(count))
    }

    /// Generate variable-size messages for realistic workloads.
    pub fn variable_messages(count: usize) -> Result<Vec<Message>> {
        let mut generator = MessageGenerator::new()
            .with_payload_range(64, 4096)
            .with_topics(vec!["bench-variable".to_string()]);

        tokio::runtime::Runtime::new()
            .expect("Failed to create runtime")
            .block_on(generator.generate_batch(count))
    }

    /// Generate a single message of specified size.
    pub fn single_message(size: usize) -> Result<Message> {
        let payload = "A".repeat(size);
        Message::new("bench-single", Bytes::from(payload))
    }

    /// Generate JSON messages for serialization benchmarks.
    pub fn json_messages(count: usize) -> Result<Vec<Message>> {
        let mut messages = Vec::with_capacity(count);
        
        for i in 0..count {
            let json_data = serde_json::json!({
                "id": i,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "user_id": format!("user_{}", i % 1000),
                "action": "benchmark_action",
                "data": {
                    "value": i * 2,
                    "category": format!("cat_{}", i % 10),
                    "metadata": {
                        "source": "benchmark",
                        "version": "1.0"
                    }
                }
            });
            
            let payload = Bytes::from(json_data.to_string());
            messages.push(Message::new("bench-json", payload)?);
        }
        
        Ok(messages)
    }
}

/// Benchmark runtime setup utilities.
pub struct BenchmarkRuntime;

impl BenchmarkRuntime {
    /// Create a tokio runtime optimized for benchmarks.
    pub fn create() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(num_cpus::get())
            .enable_all()
            .build()
            .expect("Failed to create benchmark runtime")
    }

    /// Create a single-threaded runtime for microbenchmarks.
    pub fn create_current_thread() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create current thread runtime")
    }

    /// Setup environment for performance benchmarks.
    pub fn setup_env() {
        // Disable debug logging for benchmarks
        std::env::set_var("RUST_LOG", "error");
        
        // Set optimal runtime configuration
        std::env::set_var("TOKIO_WORKER_THREADS", &num_cpus::get().to_string());
        
        // Initialize minimal tracing for benchmarks
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .without_time()
            .init();
    }
}

/// Benchmark context for setting up test scenarios.
pub struct BenchmarkContext;

impl BenchmarkContext {
    /// Create a minimal test context for microbenchmarks.
    pub async fn minimal() -> TestContext {
        let config = TestConfig {
            broker_count: 1,
            publisher_count: 1,
            consumer_count: 1,
            timeout: Duration::from_secs(10),
            validate_performance: false,
            enable_chaos: false,
            target_throughput: 1000,
            max_latency: Duration::from_millis(100),
        };

        TestContext::with_config(config).await
    }

    /// Create a context for throughput benchmarks.
    pub async fn throughput() -> TestContext {
        TestContext::with_config(BenchmarkConfig::throughput()).await
    }

    /// Create a context for latency benchmarks.
    pub async fn latency() -> TestContext {
        TestContext::with_config(BenchmarkConfig::latency()).await
    }

    /// Create a context for concurrent operation benchmarks.
    pub async fn concurrent() -> TestContext {
        TestContext::with_config(BenchmarkConfig::concurrent()).await
    }
}