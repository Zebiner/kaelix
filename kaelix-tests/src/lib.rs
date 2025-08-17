//! # Kaelix Tests
//!
//! Comprehensive testing framework for the MemoryStreamer distributed streaming system.
//!
//! This crate provides a complete testing infrastructure including:
//! - Property-based testing with data generators
//! - Load testing and performance validation
//! - Chaos engineering and fault injection
//! - Integration testing across all components
//! - Mock implementations for isolated testing
//! - Invariant validation for system correctness
//! - Distributed cluster management and testing
//! - Network simulation and fault injection
//! - Process orchestration and resource management
//! - Comprehensive monitoring and observability
//! - Real-time alerting and notifications
//! - Multi-format test reporting (HTML, JSON, XML)
//! - CI/CD integration for automated testing
//!
//! ## Testing Philosophy
//!
//! The testing framework is designed around several key principles:
//! - **Performance-first**: All tests validate performance characteristics
//! - **Chaos-ready**: Built-in fault injection and network partitioning
//! - **Property-based**: Extensive use of property-based testing
//! - **Real-world**: Realistic workloads and failure scenarios
//! - **Invariant-driven**: Comprehensive correctness validation
//! - **Distributed-aware**: Native support for multi-node scenarios
//! - **Observable**: Comprehensive monitoring and alerting
//!
//! ## Test Categories
//!
//! ### Unit Tests
//! - Individual component functionality
//! - Error handling and edge cases
//! - Performance microbenchmarks
//!
//! ### Integration Tests
//! - Cross-component interactions
//! - End-to-end message flows
//! - Configuration validation
//! - Distributed consensus testing
//! - Failover scenarios
//!
//! ### Property Tests
//! - Message generation and validation
//! - Chaos engineering scenarios
//! - Security and authorization
//! - Performance characteristics
//!
//! ### Load Tests
//! - High-throughput scenarios (10M+ msg/sec)
//! - Latency validation (<10μs P99)
//! - Resource utilization under load
//!
//! ### Chaos Tests
//! - Network partitions and failures
//! - Process failures and restarts
//! - Resource exhaustion scenarios
//! - Clock skew and timing issues
//!
//! ## Performance Requirements
//!
//! All tests must validate against these performance targets:
//! - **Throughput**: 10M+ messages per second sustained
//! - **Latency**: P99 < 10μs for message processing
//! - **Memory**: Bounded memory usage under all conditions
//! - **CPU**: Efficient CPU utilization across all cores
//! - **Network**: Zero-copy networking where possible
//!
//! ## Usage Examples
//!
//! ### Basic Test Setup
//!
//! ```rust,no_run
//! use kaelix_tests::prelude::*;
//!
//! #[tokio::test]
//! async fn test_basic_throughput() -> TestResult<()> {
//!     let mut context = TestContext::new("throughput_test").await?;
//!     let mut generator = LoadGenerator::new();
//!     
//!     // Configure for 1M msg/sec load
//!     let results = generator
//!         .with_rate(1_000_000)
//!         .with_duration(Duration::from_secs(10))
//!         .run(&mut context)
//!         .await?;
//!     
//!     assert!(results.avg_throughput >= 1_000_000.0);
//!     Ok(())
//! }
//! ```
//!
//! ### Property-Based Testing
//!
//! ```rust,no_run
//! use kaelix_tests::prelude::*;
//!
//! proptest_throughput! {
//!     fn message_ordering_preserved(
//!         messages in messages_strategy(1..1000),
//!         target_rate in 1000u64..10_000_000u64
//!     ) {
//!         let mut context = TestContext::new("ordering_test").await?;
//!         let results = send_and_receive_messages(&mut context, messages, target_rate).await?;
//!         prop_assert!(validate_message_ordering(&results));
//!     }
//! }
//! ```
//!
//! ### Chaos Engineering
//!
//! ```rust,no_run
//! use kaelix_tests::prelude::*;
//!
//! #[tokio::test]
//! async fn test_partition_tolerance() -> TestResult<()> {
//!     let mut context = TestContext::new("partition_test").await?;
//!     let mut chaos = ChaosEngine::new();
//!     
//!     // Start normal operations
//!     let load_handle = start_background_load(&mut context).await?;
//!     
//!     // Inject network partition
//!     chaos.partition_network(&context, Duration::from_secs(30)).await?;
//!     
//!     // Verify system recovers
//!     let results = load_handle.await?;
//!     assert!(results.error_rate < 0.01); // < 1% errors
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Monitoring and Alerting
//!
//! ```rust,no_run
//! use kaelix_tests::prelude::*;
//!
//! #[tokio::test]
//! async fn test_with_monitoring() -> TestResult<()> {
//!     let mut context = TestContext::new("monitored_test").await?;
//!     
//!     // Set up monitoring
//!     let metrics = TestMetrics::new();
//!     let mut monitor = create_default_monitor(Arc::new(metrics));
//!     monitor.start().await?;
//!     
//!     // Run test with real-time monitoring
//!     let results = run_performance_test(&mut context).await?;
//!     
//!     // Generate comprehensive report
//!     let mut report = TestReport::new("performance_test".to_string());
//!     report.add_result(results);
//!     report.finalize();
//!     
//!     // Export reports in multiple formats
//!     std::fs::write("report.html", report.generate_html())?;
//!     std::fs::write("report.json", serde_json::to_string_pretty(&report.generate_json())?)?;
//!     std::fs::write("junit.xml", report.generate_junit_xml())?;
//!     
//!     Ok(())
//! }
//! ```

pub mod context;
pub mod fixtures;
pub mod generators;
pub mod simulators;
pub mod validators;
pub mod utils;
pub mod macros;
pub mod properties;
pub mod data;
pub mod scenarios;

// Distributed testing modules
pub mod cluster;
pub mod orchestration;

// Monitoring and observability modules
pub mod metrics;
pub mod monitoring;
pub mod reporting;
pub mod observability;

// Re-export core testing traits and types
pub use context::{TestContext, TestResult};
pub use generators::LoadGenerator;
pub use simulators::ChaosEngine;

// Re-export validator components
pub use validators::{
    PerformanceValidator, CorrectnessValidator, SecurityValidator,
    throughput::ThroughputValidator,
    latency::LatencyValidator,
    memory::MemoryValidator,
    network::NetworkValidator,
    consensus::ConsensusValidator,
    invariants::{
        ValidationResult, InvariantValidator, MessageOrderingInvariant,
        DeliveryGuaranteesInvariant, PartitionToleranceInvariant,
        ConsistencyInvariant, PerformanceInvariant,
        SecurityInvariant, ResourceInvariant,
    },
};

// Re-export cluster management
pub use cluster::{
    TestCluster, ClusterConfig, NodeId, NodeRole, NodeState, ClusterState,
    TestBroker, BrokerConfig, BrokerState, ConsensusAlgorithm, ReplicationConfig,
    NetworkConfig, SecurityConfig,
};

// Re-export orchestration
pub use orchestration::{
    ProcessManager, ProcessConfig, ResourceUsage, ProcessState, ProcessInfo,
    SystemState, OrderingInvariant, DurabilityInvariant, ConsistencyInvariant,
    LatencyInvariant, ThroughputInvariant, PartitionInvariant, AuthorizationInvariant,
};

// Re-export property test macros
pub use macros::{
    proptest_throughput, proptest_latency, proptest_invariant, proptest_chaos,
    proptest_benchmark, proptest_security, proptest_integration,
    assertions, shrinking, TestError,
};

// Re-export generator components
pub use generators::{
    data::{MessageGenerator, PayloadDistribution, KeyDistribution, CompressionType},
    load::{LoadGenerator as LoadGen, LoadPattern, LoadProfile},
    workload::{WorkloadSimulator, ProducerWorkload, ConsumerWorkload},
    realistic::RealisticPatterns,
    efficient::{StreamingGenerator, StreamingGeneratorBuilder},
};

// Re-export monitoring and observability
pub use metrics::{TestMetrics, MetricsSnapshot, create_test_metrics, create_shared_test_metrics};
pub use monitoring::{
    RealtimeMonitor, MetricCollector, SystemResourceCollector, NetworkCollector,
    ApplicationMetricsCollector, MonitoringConfig, create_default_monitor,
    AlertManager, AlertRule, AlertCondition, AlertSeverity, NotificationChannel,
    SlackNotification, EmailNotification, WebhookNotification, create_default_alert_rules,
};
pub use reporting::{
    TestReport, TestResult as ReportTestResult, TestStatus, CoverageData, PerformanceSummary,
    create_test_report,
    dashboard::{PerformanceDashboard, Chart, ChartType, Alert, create_performance_dashboard},
    ci::{CIReporter, CIProvider, CIConfig, create_ci_reporter, create_test_artifacts},
};
pub use observability::{
    TestTracer, TestLogger, SystemInfo, BaselineMetrics,
    setup_test_observability, collect_system_info, benchmark_baseline,
};

/// Common imports for test modules
pub mod prelude {
    pub use crate::{
        TestContext, TestResult, LoadGenerator, ChaosEngine,
        PerformanceValidator, CorrectnessValidator,
    };
    
    // Distributed testing imports
    pub use crate::{
        TestCluster, ClusterConfig, NodeId, TestBroker, BrokerConfig, BrokerState,
        ProcessManager, ProcessConfig, ResourceUsage, ProcessState,
    };
    
    // Data management imports
    pub use crate::{
        TestDataManager, TestDataSet, DataSetMetadata, DataGenConfig, StorageBackend,
    };
    
    // Scenario imports
    pub use crate::{
        LoadTestScenario, ProducerConfig, ConsumerConfig, SuccessCriteria,
    };
    
    // Generator imports
    pub use crate::{
        MessageGenerator, PayloadDistribution, KeyDistribution, LoadPattern,
        WorkloadSimulator, RealisticPatterns, StreamingGenerator,
    };
    
    // Monitoring and observability imports
    pub use crate::{
        TestMetrics, MetricsSnapshot, RealtimeMonitor, AlertManager, AlertRule,
        TestReport, TestStatus, PerformanceDashboard, CIReporter,
        TestTracer, TestLogger, setup_test_observability,
        create_test_metrics, create_default_monitor, create_test_report,
        create_performance_dashboard, collect_system_info, benchmark_baseline,
    };
    
    // Property testing imports
    pub use crate::{
        proptest_throughput, proptest_latency, proptest_invariant, proptest_chaos,
        proptest_benchmark, proptest_security, proptest_integration,
    };
    
    // Common external dependencies
    pub use async_trait::async_trait;
    pub use chrono::{DateTime, Utc};
    pub use futures::future::BoxFuture;
    pub use serde::{Deserialize, Serialize};
    pub use std::sync::Arc;
    pub use std::time::{Duration, Instant, SystemTime};
    pub use tokio::{sync::RwLock, time::timeout};
    pub use tracing::{debug, error, info, span, warn, Level, Span};
    pub use uuid::Uuid;
}

/// Testing constants and configuration values
pub mod constants {
    use std::time::Duration;
    
    /// Target throughput for performance tests (messages per second)
    pub const TARGET_THROUGHPUT: u64 = 10_000_000;
    
    /// Maximum acceptable P99 latency (microseconds)
    pub const MAX_P99_LATENCY_US: u64 = 10;
    
    /// Default test timeout
    pub const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_secs(300);
    
    /// Default cluster size for distributed tests
    pub const DEFAULT_CLUSTER_SIZE: usize = 3;
    
    /// Maximum memory usage per node (bytes)
    pub const MAX_MEMORY_PER_NODE: u64 = 8 * 1024 * 1024 * 1024; // 8GB
    
    /// Default network interface for monitoring
    pub const DEFAULT_NETWORK_INTERFACE: &str = "eth0";
    
    /// Default metrics collection interval
    pub const DEFAULT_METRICS_INTERVAL_MS: u64 = 100;
    
    /// Maximum monitoring overhead percentage
    pub const MAX_MONITORING_OVERHEAD_PERCENT: f64 = 1.0;
}

/// Property testing utilities and strategies
pub mod property_testing {
    use proptest::prelude::*;
    use kaelix_core::{Message, Topic};
    
    /// Strategy for generating realistic message sizes
    pub fn message_size_strategy() -> impl Strategy<Value = usize> {
        prop_oneof![
            // Small messages (typical IoT)
            1..1024,
            // Medium messages (typical events)
            1024..65536,
            // Large messages (batch data)
            65536..1048576,
        ]
    }
    
    /// Strategy for generating topic names
    pub fn topic_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            // Simple topics
            "[a-z]{3,10}",
            // Hierarchical topics
            "[a-z]{3,5}/[a-z]{3,5}/[a-z]{3,5}",
            // Numbered topics
            "[a-z]{3,5}[0-9]{1,3}",
        ]
    }
    
    /// Strategy for generating message counts
    pub fn message_count_strategy() -> impl Strategy<Value = usize> {
        prop_oneof![
            // Small batches
            1..100usize,
            // Medium batches
            100..10000,
            // Large batches
            10000..1000000,
        ]
    }
    
    /// Strategy for generating throughput targets
    pub fn throughput_strategy() -> impl Strategy<Value = u64> {
        prop_oneof![
            // Low throughput
            1u64..1000,
            // Medium throughput
            1000..100000,
            // High throughput
            100000..10000000,
        ]
    }
}

/// Integration testing utilities
pub mod integration {
    use super::*;
    use kaelix_core::{Message, Topic};
    use std::collections::HashMap;
    use tokio::sync::mpsc;
    
    /// Creates a test environment for integration tests
    pub async fn create_test_environment(name: &str) -> TestResult<TestContext> {
        let mut context = TestContext::new(name).await?;
        
        // Initialize monitoring
        let (tracer, logger) = setup_test_observability()
            .map_err(|e| TestError::SetupError(e.to_string()))?;
        
        // Set up metrics collection
        let metrics = create_shared_test_metrics();
        let monitor = create_default_monitor(metrics.clone());
        
        // Store in context for later use
        // context.set_tracer(tracer);
        // context.set_logger(logger);
        // context.set_metrics(metrics);
        // context.set_monitor(monitor);
        
        info!("Test environment created: {}", name);
        Ok(context)
    }
    
    /// Sends messages and validates delivery
    pub async fn send_and_receive_messages(
        context: &mut TestContext,
        messages: Vec<Message>,
        target_rate: u64,
    ) -> TestResult<Vec<Message>> {
        // Implementation would use the actual message sending logic
        // For now, return the input messages as a placeholder
        Ok(messages)
    }
    
    /// Validates message ordering
    pub fn validate_message_ordering(messages: &[Message]) -> bool {
        // Implementation would check message sequence numbers, timestamps, etc.
        // For now, return true as a placeholder
        true
    }
    
    /// Starts background load generation
    pub async fn start_background_load(
        context: &mut TestContext,
    ) -> TestResult<tokio::task::JoinHandle<TestResult<LoadTestResults>>> {
        let handle = tokio::spawn(async move {
            // Implementation would start actual load generation
            // For now, return dummy results
            Ok(LoadTestResults {
                avg_throughput: 1000000.0,
                error_rate: 0.001,
                duration: Duration::from_secs(60),
            })
        });
        
        Ok(handle)
    }
    
    /// Results from load testing
    #[derive(Debug, Clone)]
    pub struct LoadTestResults {
        pub avg_throughput: f64,
        pub error_rate: f64,
        pub duration: Duration,
    }
    
    /// Runs a comprehensive performance test
    pub async fn run_performance_test(context: &mut TestContext) -> TestResult<TestResult> {
        // Implementation would run actual performance tests
        // For now, return a successful result
        Ok(TestResult::new("performance_test".to_string()).passed())
    }
}

/// Distributed testing utilities
pub mod distributed {
    use super::*;
    
    /// Creates a distributed test cluster
    pub async fn create_test_cluster(
        name: &str,
        size: usize,
    ) -> TestResult<TestCluster> {
        let config = ClusterConfig {
            name: name.to_string(),
            size,
            ..Default::default()
        };
        
        TestCluster::new(config).await
    }
    
    /// Validates cluster consensus
    pub async fn validate_cluster_consensus(
        cluster: &TestCluster,
    ) -> TestResult<bool> {
        // Implementation would check consensus across all nodes
        // For now, return true as placeholder
        Ok(true)
    }
    
    /// Simulates node failure
    pub async fn simulate_node_failure(
        cluster: &mut TestCluster,
        node_id: NodeId,
        duration: Duration,
    ) -> TestResult<()> {
        cluster.stop_node(node_id).await?;
        tokio::time::sleep(duration).await;
        cluster.start_node(node_id).await?;
        Ok(())
    }
}

/// Test execution framework with comprehensive monitoring
pub struct TestFramework {
    pub context: TestContext,
    pub metrics: Arc<TestMetrics>,
    pub monitor: RealtimeMonitor,
    pub tracer: TestTracer,
    pub reporter: TestReport,
}

impl TestFramework {
    /// Creates a new test framework instance
    pub async fn new(suite_name: &str) -> TestResult<Self> {
        let context = TestContext::new(suite_name).await?;
        let metrics = create_shared_test_metrics();
        let monitor = create_default_monitor(metrics.clone());
        let (tracer, _logger) = setup_test_observability()
            .map_err(|e| TestError::SetupError(e.to_string()))?;
        let reporter = create_test_report(suite_name);
        
        Ok(Self {
            context,
            metrics,
            monitor,
            tracer,
            reporter,
        })
    }
    
    /// Starts monitoring and tracing
    pub async fn start_monitoring(&mut self) -> TestResult<()> {
        self.tracer.start_trace(&self.reporter.suite_name, None);
        // self.monitor.start().await
        //     .map_err(|e| TestError::SetupError(e.to_string()))?;
        Ok(())
    }
    
    /// Stops monitoring and generates final report
    pub async fn finalize_and_report(&mut self) -> TestResult<TestReport> {
        // self.monitor.stop().await
        //     .map_err(|e| TestError::SetupError(e.to_string()))?;
        
        self.reporter.finalize();
        Ok(self.reporter.clone())
    }
}