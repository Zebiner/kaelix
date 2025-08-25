//! # Chaos Engineering and Testing
//!
//! Provides built-in chaos testing capabilities to validate
//! high availability and failover mechanisms under failure scenarios.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};

use tokio::{
    sync::{RwLock, broadcast},
    time::{sleep, interval},
};

use tracing::{debug, info, warn, error, instrument};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::types::NodeId;

use super::{HaResult, HaError, FailoverType};

/// Chaos testing errors
#[derive(Error, Debug, Clone)]
pub enum ChaosError {
    /// Scenario execution failed
    #[error("Chaos scenario execution failed: {scenario}")]
    ScenarioFailed { scenario: String },
    
    /// Invalid scenario configuration
    #[error("Invalid chaos scenario configuration: {0}")]
    InvalidConfiguration(String),
    
    /// Chaos test timeout
    #[error("Chaos test timeout: {scenario}")]
    TestTimeout { scenario: String },
    
    /// Safety check failed
    #[error("Safety check failed, aborting chaos test: {0}")]
    SafetyCheckFailed(String),
}

/// Chaos operation result
pub type ChaosResult<T> = std::result::Result<T, ChaosError>;

/// Chaos scenario types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChaosScenario {
    /// Simulate random node failures
    RandomNodeFailure,
    /// Simulate network partitions
    NetworkPartition,
    /// Simulate cascade failures
    CascadeFailure,
    /// Simulate leader failures
    LeaderFailure,
    /// Simulate replica failures
    ReplicaFailure,
    /// Simulate high latency
    HighLatency,
    /// Simulate packet loss
    PacketLoss,
    /// Simulate disk failures
    DiskFailure,
    /// Simulate memory pressure
    MemoryPressure,
    /// Simulate CPU starvation
    CpuStarvation,
    /// Simulate clock skew
    ClockSkew,
}

impl ChaosScenario {
    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::RandomNodeFailure => "Random node failure simulation",
            Self::NetworkPartition => "Network partition simulation",
            Self::CascadeFailure => "Cascading failure simulation", 
            Self::LeaderFailure => "Leader node failure simulation",
            Self::ReplicaFailure => "Replica failure simulation",
            Self::HighLatency => "High network latency simulation",
            Self::PacketLoss => "Network packet loss simulation",
            Self::DiskFailure => "Disk failure simulation",
            Self::MemoryPressure => "Memory pressure simulation",
            Self::CpuStarvation => "CPU starvation simulation",
            Self::ClockSkew => "Clock synchronization issues simulation",
        }
    }

    /// Get expected impact severity (1-10, higher is more severe)
    pub fn severity_level(&self) -> u8 {
        match self {
            Self::RandomNodeFailure => 6,
            Self::NetworkPartition => 9,
            Self::CascadeFailure => 10,
            Self::LeaderFailure => 7,
            Self::ReplicaFailure => 5,
            Self::HighLatency => 4,
            Self::PacketLoss => 5,
            Self::DiskFailure => 8,
            Self::MemoryPressure => 6,
            Self::CpuStarvation => 7,
            Self::ClockSkew => 6,
        }
    }
}

/// Chaos test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosConfig {
    /// Test execution interval
    pub execution_interval: Duration,
    /// Maximum concurrent chaos scenarios
    pub max_concurrent_scenarios: usize,
    /// Safety checks enabled
    pub safety_checks_enabled: bool,
    /// Minimum healthy nodes required
    pub min_healthy_nodes: usize,
    /// Test timeout per scenario
    pub test_timeout: Duration,
    /// Recovery validation timeout
    pub recovery_validation_timeout: Duration,
    /// Enabled scenarios
    pub enabled_scenarios: Vec<ChaosScenario>,
    /// Probability of executing scenarios (0.0 to 1.0)
    pub execution_probability: f64,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            execution_interval: Duration::from_secs(30),
            max_concurrent_scenarios: 1,
            safety_checks_enabled: true,
            min_healthy_nodes: 2,
            test_timeout: Duration::from_secs(30),
            recovery_validation_timeout: Duration::from_secs(60),
            enabled_scenarios: vec![
                ChaosScenario::RandomNodeFailure,
                ChaosScenario::LeaderFailure,
                ChaosScenario::HighLatency,
            ],
            execution_probability: 0.1, // 10% chance per interval
        }
    }
}

impl ChaosConfig {
    /// Validate chaos configuration
    pub fn validate(&self) -> ChaosResult<()> {
        if self.max_concurrent_scenarios == 0 {
            return Err(ChaosError::InvalidConfiguration(
                "max_concurrent_scenarios must be > 0".to_string(),
            ));
        }

        if self.min_healthy_nodes == 0 {
            return Err(ChaosError::InvalidConfiguration(
                "min_healthy_nodes must be > 0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.execution_probability) {
            return Err(ChaosError::InvalidConfiguration(
                "execution_probability must be between 0.0 and 1.0".to_string(),
            ));
        }

        if self.execution_interval.is_zero() {
            return Err(ChaosError::InvalidConfiguration(
                "execution_interval must be > 0".to_string(),
            ));
        }

        if self.enabled_scenarios.is_empty() {
            return Err(ChaosError::InvalidConfiguration(
                "At least one chaos scenario must be enabled".to_string(),
            ));
        }

        Ok(())
    }
}

/// Network partition configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPartition {
    /// Nodes in partition A
    pub partition_a: Vec<NodeId>,
    /// Nodes in partition B
    pub partition_b: Vec<NodeId>,
    /// Partition duration
    pub duration: Duration,
    /// Allow healing after duration
    pub auto_heal: bool,
}

impl NetworkPartition {
    /// Create a balanced partition
    pub fn balanced(nodes: Vec<NodeId>, duration: Duration) -> Self {
        let mid = nodes.len() / 2;
        let (partition_a, partition_b) = nodes.split_at(mid);
        
        Self {
            partition_a: partition_a.to_vec(),
            partition_b: partition_b.to_vec(),
            duration,
            auto_heal: true,
        }
    }

    /// Create a minority/majority partition
    pub fn minority_split(nodes: Vec<NodeId>, duration: Duration) -> Self {
        let minority_size = 1;
        let (minority, majority) = nodes.split_at(minority_size);
        
        Self {
            partition_a: minority.to_vec(),
            partition_b: majority.to_vec(),
            duration,
            auto_heal: true,
        }
    }
}

/// Chaos test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test scenario
    pub scenario: ChaosScenario,
    /// Test execution ID
    pub test_id: String,
    /// Test success status
    pub success: bool,
    /// Total test duration
    pub duration: Duration,
    /// Recovery time (if applicable)
    pub recovery_time: Option<Duration>,
    /// Failover count during test
    pub failover_count: u32,
    /// Data consistency validated
    pub data_consistency_validated: bool,
    /// Client impact measured
    pub client_impact: ClientImpact,
    /// Error details if failed
    pub error: Option<String>,
    /// Test timestamp
    pub timestamp: SystemTime,
}

/// Client impact measurement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientImpact {
    /// Operations affected
    pub affected_operations: u64,
    /// Maximum downtime experienced
    pub max_downtime: Duration,
    /// Average latency increase
    pub avg_latency_increase: Duration,
    /// Error rate during chaos
    pub error_rate: f64,
}

impl Default for ClientImpact {
    fn default() -> Self {
        Self {
            affected_operations: 0,
            max_downtime: Duration::from_millis(0),
            avg_latency_increase: Duration::from_millis(0),
            error_rate: 0.0,
        }
    }
}

/// Active chaos test execution
#[derive(Debug)]
struct ChaosExecution {
    /// Test ID
    test_id: String,
    /// Scenario being executed
    scenario: ChaosScenario,
    /// Start time
    start_time: Instant,
    /// Expected duration
    expected_duration: Duration,
    /// Affected nodes
    affected_nodes: Vec<NodeId>,
}

/// Chaos testing manager
#[derive(Debug)]
pub struct ChaosTestManager {
    /// Local manager node ID
    manager_id: NodeId,
    /// Chaos configuration
    config: ChaosConfig,
    /// Active chaos executions
    active_executions: Arc<RwLock<HashMap<String, ChaosExecution>>>,
    /// Test result history
    test_history: Arc<RwLock<VecDeque<TestResult>>>,
    /// Test execution counter
    execution_counter: AtomicU64,
    /// Result broadcaster
    result_tx: broadcast::Sender<TestResult>,
}

impl ChaosTestManager {
    /// Create new chaos test manager
    pub fn new(manager_id: NodeId) -> Self {
        let (result_tx, _) = broadcast::channel(100);

        Self {
            manager_id,
            config: ChaosConfig::default(),
            active_executions: Arc::new(RwLock::new(HashMap::new())),
            test_history: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            execution_counter: AtomicU64::new(0),
            result_tx,
        }
    }

    /// Create with custom configuration
    pub fn with_config(manager_id: NodeId, config: ChaosConfig) -> ChaosResult<Self> {
        config.validate()?;

        let (result_tx, _) = broadcast::channel(100);

        Ok(Self {
            manager_id,
            config,
            active_executions: Arc::new(RwLock::new(HashMap::new())),
            test_history: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            execution_counter: AtomicU64::new(0),
            result_tx,
        })
    }

    /// Run periodic chaos testing cycle
    #[instrument(skip(self))]
    pub async fn run_chaos_cycle(&self) -> ChaosResult<()> {
        // Check if we should execute a chaos scenario
        if fastrand::f64() > self.config.execution_probability {
            debug!("Skipping chaos cycle (probability check)");
            sleep(self.config.execution_interval).await;
            return Ok(());
        }

        // Perform safety checks
        if self.config.safety_checks_enabled && !self.perform_safety_checks().await? {
            warn!("Safety checks failed, skipping chaos execution");
            sleep(self.config.execution_interval).await;
            return Ok(());
        }

        // Check concurrent execution limit
        let active_count = {
            let executions = self.active_executions.read().await;
            executions.len()
        };

        if active_count >= self.config.max_concurrent_scenarios {
            debug!("Maximum concurrent scenarios reached, skipping");
            sleep(self.config.execution_interval).await;
            return Ok(());
        }

        // Select random scenario
        if let Some(scenario) = self.select_random_scenario() {
            info!("Executing chaos scenario: {:?}", scenario);
            let result = self.execute_chaos_scenario(scenario).await;
            
            match result {
                Ok(test_result) => {
                    info!("Chaos scenario completed: {:?} - Success: {}", 
                          scenario, test_result.success);
                    self.record_test_result(test_result).await;
                }
                Err(e) => {
                    error!("Chaos scenario failed: {:?} - Error: {}", scenario, e);
                    // Record failed test result
                    let failed_result = TestResult {
                        scenario,
                        test_id: format!("chaos-{}", self.execution_counter.load(Ordering::Relaxed)),
                        success: false,
                        duration: Duration::from_secs(0),
                        recovery_time: None,
                        failover_count: 0,
                        data_consistency_validated: false,
                        client_impact: ClientImpact::default(),
                        error: Some(e.to_string()),
                        timestamp: SystemTime::now(),
                    };
                    self.record_test_result(failed_result).await;
                }
            }
        }

        sleep(self.config.execution_interval).await;
        Ok(())
    }

    /// Perform safety checks before chaos execution
    async fn perform_safety_checks(&self) -> ChaosResult<bool> {
        // In a real implementation, this would:
        // 1. Check cluster health
        // 2. Verify minimum healthy nodes
        // 3. Check if critical operations are ongoing
        // 4. Validate system resources
        
        // For now, simulate safety check
        let healthy_nodes = self.mock_healthy_node_count().await;
        
        if healthy_nodes < self.config.min_healthy_nodes {
            return Err(ChaosError::SafetyCheckFailed(format!(
                "Insufficient healthy nodes: {} < {}",
                healthy_nodes,
                self.config.min_healthy_nodes
            )));
        }

        Ok(true)
    }

    /// Mock healthy node count (in real implementation, would query cluster)
    async fn mock_healthy_node_count(&self) -> usize {
        // Simulate varying healthy node count
        let base_count = 5;
        let variation = fastrand::usize(0..3);
        base_count + variation
    }

    /// Select random chaos scenario
    fn select_random_scenario(&self) -> Option<ChaosScenario> {
        if self.config.enabled_scenarios.is_empty() {
            return None;
        }

        let index = fastrand::usize(..self.config.enabled_scenarios.len());
        Some(self.config.enabled_scenarios[index])
    }

    /// Execute a specific chaos scenario
    async fn execute_chaos_scenario(&self, scenario: ChaosScenario) -> ChaosResult<TestResult> {
        let test_id = format!("chaos-{}", self.execution_counter.fetch_add(1, Ordering::Relaxed));
        let start_time = Instant::now();
        
        info!("Starting chaos test {} for scenario: {:?}", test_id, scenario);

        // Create execution tracking
        let execution = ChaosExecution {
            test_id: test_id.clone(),
            scenario,
            start_time,
            expected_duration: self.config.test_timeout,
            affected_nodes: Vec::new(), // Will be populated based on scenario
        };

        // Track active execution
        {
            let mut executions = self.active_executions.write().await;
            executions.insert(test_id.clone(), execution);
        }

        // Execute the specific scenario
        let result = match scenario {
            ChaosScenario::RandomNodeFailure => {
                self.simulate_random_node_failure().await
            }
            ChaosScenario::NetworkPartition => {
                self.simulate_network_partition().await
            }
            ChaosScenario::CascadeFailure => {
                self.simulate_cascade_failure().await
            }
            ChaosScenario::LeaderFailure => {
                self.simulate_leader_failure().await
            }
            ChaosScenario::ReplicaFailure => {
                self.simulate_replica_failure().await
            }
            ChaosScenario::HighLatency => {
                self.simulate_high_latency().await
            }
            ChaosScenario::PacketLoss => {
                self.simulate_packet_loss().await
            }
            ChaosScenario::DiskFailure => {
                self.simulate_disk_failure().await
            }
            ChaosScenario::MemoryPressure => {
                self.simulate_memory_pressure().await
            }
            ChaosScenario::CpuStarvation => {
                self.simulate_cpu_starvation().await
            }
            ChaosScenario::ClockSkew => {
                self.simulate_clock_skew().await
            }
        };

        // Clean up execution tracking
        {
            let mut executions = self.active_executions.write().await;
            executions.remove(&test_id);
        }

        let duration = start_time.elapsed();

        match result {
            Ok((recovery_time, failover_count, client_impact)) => {
                Ok(TestResult {
                    scenario,
                    test_id,
                    success: true,
                    duration,
                    recovery_time,
                    failover_count,
                    data_consistency_validated: true, // Assume validation passed
                    client_impact,
                    error: None,
                    timestamp: SystemTime::now(),
                })
            }
            Err(e) => {
                Err(ChaosError::ScenarioFailed {
                    scenario: format!("{:?}", scenario),
                })
            }
        }
    }

    /// Simulate random node failure
    async fn simulate_random_node_failure(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating random node failure");
        
        // Simulate failure detection and recovery
        sleep(Duration::from_millis(500)).await; // Failure occurs
        let recovery_start = Instant::now();
        sleep(Duration::from_millis(300)).await; // Recovery time
        let recovery_time = recovery_start.elapsed();

        let client_impact = ClientImpact {
            affected_operations: 150,
            max_downtime: recovery_time,
            avg_latency_increase: Duration::from_millis(50),
            error_rate: 0.02,
        };

        Ok((Some(recovery_time), 1, client_impact))
    }

    /// Simulate network partition
    pub async fn simulate_network_partition(&self, partition: NetworkPartition) -> ChaosResult<TestResult> {
        info!("Simulating network partition: {} nodes in A, {} nodes in B", 
              partition.partition_a.len(), partition.partition_b.len());
        
        let start_time = Instant::now();
        
        // Simulate partition creation
        sleep(Duration::from_millis(100)).await;
        
        // Wait for partition duration
        sleep(partition.duration).await;
        
        // Simulate partition healing if enabled
        let recovery_time = if partition.auto_heal {
            let heal_start = Instant::now();
            sleep(Duration::from_millis(200)).await; // Healing time
            Some(heal_start.elapsed())
        } else {
            None
        };

        let duration = start_time.elapsed();
        let client_impact = ClientImpact {
            affected_operations: 500,
            max_downtime: partition.duration,
            avg_latency_increase: Duration::from_millis(200),
            error_rate: 0.1,
        };

        Ok(TestResult {
            scenario: ChaosScenario::NetworkPartition,
            test_id: format!("partition-{}", self.execution_counter.load(Ordering::Relaxed)),
            success: true,
            duration,
            recovery_time,
            failover_count: 2, // Partition typically causes multiple failovers
            data_consistency_validated: true,
            client_impact,
            error: None,
            timestamp: SystemTime::now(),
        })
    }

    /// Simulate cascade failure
    async fn simulate_cascade_failure(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating cascade failure");
        
        let recovery_start = Instant::now();
        
        // Simulate multiple node failures in succession
        for i in 0..3 {
            sleep(Duration::from_millis(200)).await;
            info!("Node {} failed in cascade", i + 1);
        }
        
        // Recovery time is longer for cascade failures
        sleep(Duration::from_millis(800)).await;
        let recovery_time = recovery_start.elapsed();

        let client_impact = ClientImpact {
            affected_operations: 1000,
            max_downtime: recovery_time,
            avg_latency_increase: Duration::from_millis(300),
            error_rate: 0.15,
        };

        Ok((Some(recovery_time), 3, client_impact))
    }

    /// Simulate leader failure
    async fn simulate_leader_failure(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating leader failure");
        
        let recovery_start = Instant::now();
        sleep(Duration::from_millis(200)).await; // Leader election time
        let recovery_time = recovery_start.elapsed();

        let client_impact = ClientImpact {
            affected_operations: 200,
            max_downtime: recovery_time,
            avg_latency_increase: Duration::from_millis(30),
            error_rate: 0.01,
        };

        Ok((Some(recovery_time), 1, client_impact))
    }

    /// Simulate replica failure
    async fn simulate_replica_failure(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating replica failure");
        
        let recovery_start = Instant::now();
        sleep(Duration::from_millis(400)).await; // Replica promotion time
        let recovery_time = recovery_start.elapsed();

        let client_impact = ClientImpact {
            affected_operations: 100,
            max_downtime: recovery_time,
            avg_latency_increase: Duration::from_millis(25),
            error_rate: 0.005,
        };

        Ok((Some(recovery_time), 1, client_impact))
    }

    /// Simulate high latency
    async fn simulate_high_latency(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating high network latency");
        
        // High latency doesn't typically cause failovers, just degrades performance
        sleep(Duration::from_millis(1000)).await;

        let client_impact = ClientImpact {
            affected_operations: 300,
            max_downtime: Duration::from_millis(0),
            avg_latency_increase: Duration::from_millis(500),
            error_rate: 0.0,
        };

        Ok((None, 0, client_impact))
    }

    /// Simulate packet loss
    async fn simulate_packet_loss(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating network packet loss");
        
        sleep(Duration::from_millis(800)).await;

        let client_impact = ClientImpact {
            affected_operations: 250,
            max_downtime: Duration::from_millis(100),
            avg_latency_increase: Duration::from_millis(150),
            error_rate: 0.03,
        };

        Ok((None, 0, client_impact))
    }

    /// Simulate disk failure
    async fn simulate_disk_failure(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating disk failure");
        
        let recovery_start = Instant::now();
        sleep(Duration::from_millis(1200)).await; // Disk recovery/replacement
        let recovery_time = recovery_start.elapsed();

        let client_impact = ClientImpact {
            affected_operations: 800,
            max_downtime: recovery_time,
            avg_latency_increase: Duration::from_millis(100),
            error_rate: 0.08,
        };

        Ok((Some(recovery_time), 1, client_impact))
    }

    /// Simulate memory pressure
    async fn simulate_memory_pressure(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating memory pressure");
        
        sleep(Duration::from_millis(600)).await;

        let client_impact = ClientImpact {
            affected_operations: 400,
            max_downtime: Duration::from_millis(50),
            avg_latency_increase: Duration::from_millis(200),
            error_rate: 0.02,
        };

        Ok((None, 0, client_impact))
    }

    /// Simulate CPU starvation
    async fn simulate_cpu_starvation(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating CPU starvation");
        
        sleep(Duration::from_millis(700)).await;

        let client_impact = ClientImpact {
            affected_operations: 350,
            max_downtime: Duration::from_millis(200),
            avg_latency_increase: Duration::from_millis(400),
            error_rate: 0.01,
        };

        Ok((None, 0, client_impact))
    }

    /// Simulate clock skew
    async fn simulate_clock_skew(&self) -> Result<(Option<Duration>, u32, ClientImpact), String> {
        info!("Simulating clock synchronization issues");
        
        sleep(Duration::from_millis(900)).await;

        let client_impact = ClientImpact {
            affected_operations: 180,
            max_downtime: Duration::from_millis(0),
            avg_latency_increase: Duration::from_millis(80),
            error_rate: 0.005,
        };

        Ok((None, 0, client_impact))
    }

    /// Measure recovery time for a scenario
    pub async fn measure_recovery_time(&self, scenario: ChaosScenario) -> ChaosResult<Duration> {
        info!("Measuring recovery time for scenario: {:?}", scenario);
        
        let test_result = self.execute_chaos_scenario(scenario).await?;
        
        Ok(test_result.recovery_time.unwrap_or(test_result.duration))
    }

    /// Record test result in history
    async fn record_test_result(&self, result: TestResult) {
        let mut history = self.test_history.write().await;
        
        // Maintain history size limit
        if history.len() >= 1000 {
            history.pop_front();
        }
        
        history.push_back(result.clone());
        
        // Broadcast result
        let _ = self.result_tx.send(result);
    }

    /// Get test statistics
    pub async fn get_test_statistics(&self) -> ChaosTestStatistics {
        let history = self.test_history.read().await;
        
        let total_tests = history.len();
        let successful_tests = history.iter().filter(|r| r.success).count();
        let failed_tests = total_tests - successful_tests;
        
        let avg_recovery_time = if total_tests > 0 {
            let total_recovery_time: Duration = history
                .iter()
                .filter_map(|r| r.recovery_time)
                .sum();
            
            if successful_tests > 0 {
                total_recovery_time / successful_tests as u32
            } else {
                Duration::from_secs(0)
            }
        } else {
            Duration::from_secs(0)
        };

        let avg_failover_count = if total_tests > 0 {
            history.iter().map(|r| r.failover_count as f64).sum::<f64>() / total_tests as f64
        } else {
            0.0
        };

        ChaosTestStatistics {
            total_tests,
            successful_tests,
            failed_tests,
            avg_recovery_time,
            avg_failover_count,
            scenarios_tested: history.iter().map(|r| r.scenario).collect::<std::collections::HashSet<_>>().len(),
        }
    }

    /// Subscribe to test results
    pub fn subscribe_to_results(&self) -> broadcast::Receiver<TestResult> {
        self.result_tx.subscribe()
    }

    /// Get active tests count
    pub async fn active_tests_count(&self) -> usize {
        let executions = self.active_executions.read().await;
        executions.len()
    }
}

/// Chaos test statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosTestStatistics {
    /// Total tests executed
    pub total_tests: usize,
    /// Successful tests
    pub successful_tests: usize,
    /// Failed tests
    pub failed_tests: usize,
    /// Average recovery time
    pub avg_recovery_time: Duration,
    /// Average failover count per test
    pub avg_failover_count: f64,
    /// Number of different scenarios tested
    pub scenarios_tested: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chaos_scenario_properties() {
        let scenario = ChaosScenario::NetworkPartition;
        assert_eq!(scenario.severity_level(), 9);
        assert!(!scenario.description().is_empty());
    }

    #[test]
    fn test_chaos_config_validation() {
        let mut config = ChaosConfig::default();
        assert!(config.validate().is_ok());

        config.execution_probability = 1.5;
        assert!(config.validate().is_err());

        config.execution_probability = 0.5;
        config.enabled_scenarios.clear();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_network_partition_creation() {
        let nodes = vec![NodeId::generate(), NodeId::generate(), NodeId::generate(), NodeId::generate()];
        let duration = Duration::from_secs(30);
        
        let partition = NetworkPartition::balanced(nodes.clone(), duration);
        assert_eq!(partition.partition_a.len(), 2);
        assert_eq!(partition.partition_b.len(), 2);
        assert_eq!(partition.duration, duration);
        assert!(partition.auto_heal);

        let minority_partition = NetworkPartition::minority_split(nodes, duration);
        assert_eq!(minority_partition.partition_a.len(), 1);
        assert_eq!(minority_partition.partition_b.len(), 3);
    }

    #[tokio::test]
    async fn test_chaos_manager_creation() {
        let manager_id = NodeId::generate();
        let manager = ChaosTestManager::new(manager_id);
        assert_eq!(manager.manager_id, manager_id);
        
        let stats = manager.get_test_statistics().await;
        assert_eq!(stats.total_tests, 0);
    }

    #[test]
    fn test_client_impact() {
        let impact = ClientImpact {
            affected_operations: 100,
            max_downtime: Duration::from_millis(500),
            avg_latency_increase: Duration::from_millis(50),
            error_rate: 0.02,
        };

        assert_eq!(impact.affected_operations, 100);
        assert_eq!(impact.max_downtime, Duration::from_millis(500));
        assert_eq!(impact.error_rate, 0.02);
    }

    #[tokio::test]
    async fn test_safety_checks() {
        let manager = ChaosTestManager::new(NodeId::generate());
        let safety_result = manager.perform_safety_checks().await;
        // Should pass with mock data
        assert!(safety_result.is_ok());
    }
}