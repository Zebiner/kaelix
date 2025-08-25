//! # Recovery Management
//!
//! Orchestrates cluster recovery operations, data reconciliation,
//! and cluster rebalancing after failures and failovers.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};

use tokio::{
    sync::{RwLock, Mutex, broadcast},
    time::{sleep, timeout},
};

use tracing::{debug, info, warn, error, instrument};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::types::NodeId;

use super::{HaResult, HaError};

/// Recovery operation errors
#[derive(Error, Debug, Clone)]
pub enum RecoveryError {
    /// Data synchronization failed
    #[error("Data synchronization failed for node {node_id}: {reason}")]
    DataSyncFailed { node_id: NodeId, reason: String },
    
    /// Recovery validation failed
    #[error("Recovery validation failed: {0}")]
    ValidationFailed(String),
    
    /// Recovery timeout
    #[error("Recovery operation timeout: {operation}")]
    RecoveryTimeout { operation: String },
    
    /// Insufficient healthy nodes for recovery
    #[error("Insufficient healthy nodes for recovery: need {needed}, have {available}")]
    InsufficientNodes { needed: usize, available: usize },
    
    /// Recovery conflict
    #[error("Recovery conflict: {0}")]
    ConflictDetected(String),
}

/// Recovery operation result
pub type RecoveryResult<T> = std::result::Result<T, RecoveryError>;

/// Recovery strategy types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Conservative recovery with data validation
    Conservative,
    /// Fast recovery prioritizing availability
    Fast,
    /// Full rebuild from backup/replicas
    Rebuild,
    /// Partial recovery of specific components
    Partial,
    /// Manual recovery requiring intervention
    Manual,
}

impl Default for RecoveryStrategy {
    fn default() -> Self {
        Self::Conservative
    }
}

/// Recovery plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPlan {
    /// Plan identifier
    pub plan_id: String,
    /// Nodes being recovered
    pub target_nodes: Vec<NodeId>,
    /// Recovery strategy
    pub strategy: RecoveryStrategy,
    /// Recovery steps
    pub steps: Vec<RecoveryStep>,
    /// Expected duration
    pub expected_duration: Duration,
    /// Plan creation timestamp
    pub created_at: SystemTime,
    /// Recovery coordinator
    pub coordinator: NodeId,
}

impl RecoveryPlan {
    /// Generate unique plan ID
    fn generate_plan_id() -> String {
        format!("recovery-{}", uuid::Uuid::new_v4())
    }

    /// Create new recovery plan
    pub fn new(
        target_nodes: Vec<NodeId>,
        strategy: RecoveryStrategy,
        coordinator: NodeId,
    ) -> Self {
        let mut steps = Vec::new();
        let expected_duration;

        match strategy {
            RecoveryStrategy::Conservative => {
                steps.push(RecoveryStep::ValidateClusterState);
                steps.push(RecoveryStep::PrepareRecovery);
                steps.push(RecoveryStep::SynchronizeData);
                steps.push(RecoveryStep::ValidateDataConsistency);
                steps.push(RecoveryStep::RebalanceCluster);
                steps.push(RecoveryStep::ValidateRecovery);
                expected_duration = Duration::from_secs(30);
            }
            RecoveryStrategy::Fast => {
                steps.push(RecoveryStep::PrepareRecovery);
                steps.push(RecoveryStep::RestoreFromReplica);
                steps.push(RecoveryStep::RebalanceCluster);
                steps.push(RecoveryStep::ValidateRecovery);
                expected_duration = Duration::from_secs(10);
            }
            RecoveryStrategy::Rebuild => {
                steps.push(RecoveryStep::ValidateClusterState);
                steps.push(RecoveryStep::PrepareRecovery);
                steps.push(RecoveryStep::RebuildFromBackup);
                steps.push(RecoveryStep::SynchronizeData);
                steps.push(RecoveryStep::ValidateDataConsistency);
                steps.push(RecoveryStep::RebalanceCluster);
                steps.push(RecoveryStep::ValidateRecovery);
                expected_duration = Duration::from_secs(60);
            }
            RecoveryStrategy::Partial => {
                steps.push(RecoveryStep::PrepareRecovery);
                steps.push(RecoveryStep::PartialRestore);
                steps.push(RecoveryStep::ValidateRecovery);
                expected_duration = Duration::from_secs(15);
            }
            RecoveryStrategy::Manual => {
                steps.push(RecoveryStep::PrepareRecovery);
                steps.push(RecoveryStep::WaitForManualIntervention);
                steps.push(RecoveryStep::ValidateRecovery);
                expected_duration = Duration::from_secs(300); // 5 minutes for manual
            }
        }

        Self {
            plan_id: Self::generate_plan_id(),
            target_nodes,
            strategy,
            steps,
            expected_duration,
            created_at: SystemTime::now(),
            coordinator,
        }
    }
}

/// Individual recovery step
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryStep {
    /// Validate current cluster state
    ValidateClusterState,
    /// Prepare nodes for recovery
    PrepareRecovery,
    /// Synchronize data between nodes
    SynchronizeData,
    /// Validate data consistency
    ValidateDataConsistency,
    /// Restore from healthy replicas
    RestoreFromReplica,
    /// Rebuild from backup data
    RebuildFromBackup,
    /// Perform partial restoration
    PartialRestore,
    /// Rebalance cluster load
    RebalanceCluster,
    /// Validate recovery completion
    ValidateRecovery,
    /// Wait for manual intervention
    WaitForManualIntervention,
}

/// Recovery step result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStepResult {
    /// Step that was executed
    pub step: RecoveryStep,
    /// Success status
    pub success: bool,
    /// Execution duration
    pub duration: Duration,
    /// Data processed (bytes)
    pub data_processed: u64,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp of execution
    pub timestamp: SystemTime,
}

/// Recovery execution state
#[derive(Debug)]
pub struct RecoveryExecution {
    /// Associated plan
    pub plan: RecoveryPlan,
    /// Current step index
    pub current_step: usize,
    /// Step results
    pub step_results: Vec<RecoveryStepResult>,
    /// Start time
    pub start_time: Instant,
    /// Total data processed
    pub total_data_processed: u64,
}

/// Recovery statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStats {
    /// Total recoveries performed
    pub total_recoveries: u64,
    /// Successful recoveries
    pub successful_recoveries: u64,
    /// Failed recoveries
    pub failed_recoveries: u64,
    /// Average recovery time
    pub average_recovery_time: Duration,
    /// Total data recovered (bytes)
    pub total_data_recovered: u64,
}

/// Recovery manager
#[derive(Debug)]
pub struct RecoveryManager {
    /// Local manager node ID
    manager_id: NodeId,
    /// Recovery validation timeout
    validation_timeout: Duration,
    /// Active recovery executions
    active_recoveries: Arc<RwLock<HashMap<String, RecoveryExecution>>>,
    /// Recovery statistics
    stats: Arc<RwLock<RecoveryStats>>,
    /// Execution counter
    execution_counter: AtomicU64,
    /// Result broadcaster
    result_tx: broadcast::Sender<RecoveryResult<()>>,
}

impl RecoveryManager {
    /// Create new recovery manager
    pub fn new(manager_id: NodeId, validation_timeout: Duration) -> Self {
        let (result_tx, _) = broadcast::channel(100);

        Self {
            manager_id,
            validation_timeout,
            active_recoveries: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(RecoveryStats {
                total_recoveries: 0,
                successful_recoveries: 0,
                failed_recoveries: 0,
                average_recovery_time: Duration::from_secs(0),
                total_data_recovered: 0,
            })),
            execution_counter: AtomicU64::new(0),
            result_tx,
        }
    }

    /// Create recovery plan for failed nodes
    #[instrument(skip(self))]
    pub async fn create_recovery_plan(
        &self,
        target_nodes: Vec<NodeId>,
        strategy: RecoveryStrategy,
    ) -> RecoveryResult<RecoveryPlan> {
        info!("Creating recovery plan for {} nodes (strategy: {:?})", target_nodes.len(), strategy);

        if target_nodes.is_empty() {
            return Err(RecoveryError::ValidationFailed(
                "No target nodes specified for recovery".to_string(),
            ));
        }

        let plan = RecoveryPlan::new(target_nodes.clone(), strategy, self.manager_id);

        info!(
            "Created recovery plan {} for nodes {:?}, {} steps, expected duration: {:?}",
            plan.plan_id,
            target_nodes,
            plan.steps.len(),
            plan.expected_duration
        );

        Ok(plan)
    }

    /// Execute recovery plan
    #[instrument(skip(self, plan))]
    pub async fn execute_recovery(&self, plan: RecoveryPlan) -> RecoveryResult<()> {
        let execution_id = self.execution_counter.fetch_add(1, Ordering::Relaxed);
        info!("Executing recovery plan {} (execution {})", plan.plan_id, execution_id);

        let start_time = Instant::now();

        // Create execution state
        let execution = RecoveryExecution {
            plan: plan.clone(),
            current_step: 0,
            step_results: Vec::new(),
            start_time,
            total_data_processed: 0,
        };

        // Store execution
        {
            let mut recoveries = self.active_recoveries.write().await;
            recoveries.insert(plan.plan_id.clone(), execution);
        }

        // Execute steps with timeout
        let execution_future = self.execute_recovery_steps(plan.clone());
        let result = timeout(plan.expected_duration * 2, execution_future).await;

        // Clean up execution and update stats
        let final_stats = {
            let mut recoveries = self.active_recoveries.write().await;
            recoveries.remove(&plan.plan_id)
        };

        // Update statistics
        self.update_recovery_stats(start_time.elapsed(), result.is_ok(), final_stats).await;

        match result {
            Ok(Ok(())) => {
                let elapsed = start_time.elapsed();
                info!("Recovery plan {} completed successfully in {:?}", plan.plan_id, elapsed);
                let _ = self.result_tx.send(Ok(()));
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Recovery plan {} failed: {}", plan.plan_id, e);
                let _ = self.result_tx.send(Err(e.clone()));
                Err(e)
            }
            Err(_) => {
                error!("Recovery plan {} timed out", plan.plan_id);
                let error = RecoveryError::RecoveryTimeout {
                    operation: "recovery_execution".to_string(),
                };
                let _ = self.result_tx.send(Err(error.clone()));
                Err(error)
            }
        }
    }

    /// Execute all steps in the recovery plan
    async fn execute_recovery_steps(&self, plan: RecoveryPlan) -> RecoveryResult<()> {
        for (step_index, step) in plan.steps.iter().enumerate() {
            let step_start = Instant::now();
            debug!("Executing recovery step {} of {}: {:?}", step_index + 1, plan.steps.len(), step);

            let step_result = self.execute_single_recovery_step(step, &plan).await;
            let step_duration = step_start.elapsed();

            let success = step_result.is_ok();
            let error = step_result.as_ref().err().map(|e| e.to_string());
            let data_processed = step_result.unwrap_or(0);

            // Record step result
            let result = RecoveryStepResult {
                step: step.clone(),
                success,
                duration: step_duration,
                data_processed,
                error,
                timestamp: SystemTime::now(),
            };

            // Update execution state
            {
                let mut recoveries = self.active_recoveries.write().await;
                if let Some(execution) = recoveries.get_mut(&plan.plan_id) {
                    execution.current_step = step_index + 1;
                    execution.step_results.push(result);
                    execution.total_data_processed += data_processed;
                }
            }

            // Fail fast on error (except for manual intervention)
            if let Err(e) = step_result {
                if !matches!(step, RecoveryStep::WaitForManualIntervention) {
                    error!("Recovery step {:?} failed: {}", step, e);
                    return Err(RecoveryError::ValidationFailed(e.to_string()));
                }
            }

            debug!("Recovery step {:?} completed in {:?}, processed {} bytes", 
                   step, step_duration, data_processed);
        }

        Ok(())
    }

    /// Execute a single recovery step
    async fn execute_single_recovery_step(
        &self,
        step: &RecoveryStep,
        plan: &RecoveryPlan,
    ) -> Result<u64, String> {
        match step {
            RecoveryStep::ValidateClusterState => {
                self.validate_cluster_state(&plan.target_nodes).await
            }
            RecoveryStep::PrepareRecovery => {
                self.prepare_nodes_for_recovery(&plan.target_nodes).await
            }
            RecoveryStep::SynchronizeData => {
                self.synchronize_data_between_nodes(&plan.target_nodes).await
            }
            RecoveryStep::ValidateDataConsistency => {
                self.validate_data_consistency(&plan.target_nodes).await
            }
            RecoveryStep::RestoreFromReplica => {
                self.restore_from_healthy_replicas(&plan.target_nodes).await
            }
            RecoveryStep::RebuildFromBackup => {
                self.rebuild_from_backup(&plan.target_nodes).await
            }
            RecoveryStep::PartialRestore => {
                self.perform_partial_restore(&plan.target_nodes).await
            }
            RecoveryStep::RebalanceCluster => {
                self.rebalance_cluster_load(&plan.target_nodes).await
            }
            RecoveryStep::ValidateRecovery => {
                self.validate_recovery_completion(&plan.target_nodes).await
            }
            RecoveryStep::WaitForManualIntervention => {
                self.wait_for_manual_intervention().await
            }
        }
    }

    /// Validate cluster state before recovery
    async fn validate_cluster_state(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Validating cluster state for {} nodes", target_nodes.len());
        sleep(Duration::from_millis(100)).await;

        // Simulate cluster state validation
        // In real implementation: check quorum, data integrity, network connectivity
        
        Ok(0) // No data processed in validation
    }

    /// Prepare nodes for recovery operations
    async fn prepare_nodes_for_recovery(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Preparing {} nodes for recovery", target_nodes.len());
        sleep(Duration::from_millis(200)).await;

        // Simulate node preparation
        // In real implementation: stop services, backup current state, prepare storage
        
        Ok(0) // No data processed in preparation
    }

    /// Synchronize data between nodes
    async fn synchronize_data_between_nodes(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Synchronizing data for {} nodes", target_nodes.len());
        sleep(Duration::from_millis(500)).await;

        // Simulate data synchronization
        let data_synchronized = target_nodes.len() as u64 * 1024 * 1024; // 1MB per node
        
        Ok(data_synchronized)
    }

    /// Validate data consistency across nodes
    async fn validate_data_consistency(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Validating data consistency for {} nodes", target_nodes.len());
        sleep(Duration::from_millis(300)).await;

        // Simulate consistency validation
        // In real implementation: checksum validation, merkle tree comparison, etc.
        
        Ok(0) // No data processed in validation
    }

    /// Restore data from healthy replicas
    async fn restore_from_healthy_replicas(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Restoring {} nodes from healthy replicas", target_nodes.len());
        sleep(Duration::from_millis(1000)).await;

        // Simulate replica restoration
        let data_restored = target_nodes.len() as u64 * 2 * 1024 * 1024; // 2MB per node
        
        Ok(data_restored)
    }

    /// Rebuild nodes from backup data
    async fn rebuild_from_backup(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Rebuilding {} nodes from backup", target_nodes.len());
        sleep(Duration::from_millis(2000)).await;

        // Simulate backup restoration
        let data_restored = target_nodes.len() as u64 * 5 * 1024 * 1024; // 5MB per node
        
        Ok(data_restored)
    }

    /// Perform partial restoration
    async fn perform_partial_restore(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Performing partial restore for {} nodes", target_nodes.len());
        sleep(Duration::from_millis(600)).await;

        // Simulate partial restoration
        let data_restored = target_nodes.len() as u64 * 512 * 1024; // 512KB per node
        
        Ok(data_restored)
    }

    /// Rebalance cluster load after recovery
    async fn rebalance_cluster_load(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Rebalancing cluster load for {} recovered nodes", target_nodes.len());
        sleep(Duration::from_millis(400)).await;

        // Simulate load rebalancing
        // In real implementation: redistribute data, update routing, migrate connections
        
        Ok(0) // No data processed in rebalancing
    }

    /// Validate recovery completion
    async fn validate_recovery_completion(&self, target_nodes: &[NodeId]) -> Result<u64, String> {
        info!("Validating recovery completion for {} nodes", target_nodes.len());
        sleep(Duration::from_millis(200)).await;

        // Simulate recovery validation
        // In real implementation: health checks, functionality tests, performance validation
        
        Ok(0) // No data processed in validation
    }

    /// Wait for manual intervention
    async fn wait_for_manual_intervention(&self) -> Result<u64, String> {
        warn!("Waiting for manual intervention - recovery requires human attention");
        
        // In real implementation, this would wait for an operator signal
        // For testing, we'll simulate a short wait
        sleep(Duration::from_millis(1000)).await;
        
        Ok(0) // No data processed while waiting
    }

    /// Update recovery statistics
    async fn update_recovery_stats(
        &self,
        duration: Duration,
        success: bool,
        execution: Option<RecoveryExecution>,
    ) {
        let mut stats = self.stats.write().await;
        
        stats.total_recoveries += 1;
        if success {
            stats.successful_recoveries += 1;
        } else {
            stats.failed_recoveries += 1;
        }

        // Update average recovery time
        let total_time = stats.average_recovery_time * (stats.total_recoveries - 1) + duration;
        stats.average_recovery_time = total_time / stats.total_recoveries;

        // Update total data recovered
        if let Some(exec) = execution {
            stats.total_data_recovered += exec.total_data_processed;
        }
    }

    /// Run periodic recovery cycle (for background task)
    pub async fn run_recovery_cycle(&self) -> RecoveryResult<()> {
        // Monitor active recoveries and clean up completed ones
        let mut completed_recoveries = Vec::new();
        
        {
            let recoveries = self.active_recoveries.read().await;
            for (plan_id, execution) in recoveries.iter() {
                // Check if recovery has exceeded expected duration significantly
                if execution.start_time.elapsed() > execution.plan.expected_duration * 3 {
                    completed_recoveries.push(plan_id.clone());
                }
            }
        }

        // Clean up timed out recoveries
        if !completed_recoveries.is_empty() {
            let mut recoveries = self.active_recoveries.write().await;
            for plan_id in completed_recoveries {
                warn!("Cleaning up stalled recovery: {}", plan_id);
                recoveries.remove(&plan_id);
            }
        }

        // Yield to prevent busy loop
        sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    /// Get recovery statistics
    pub async fn get_stats(&self) -> RecoveryStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Get active recoveries count
    pub async fn active_recoveries_count(&self) -> usize {
        let recoveries = self.active_recoveries.read().await;
        recoveries.len()
    }

    /// Subscribe to recovery results
    pub fn subscribe_to_results(&self) -> broadcast::Receiver<RecoveryResult<()>> {
        self.result_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_plan_creation() {
        let target_nodes = vec![NodeId::generate(), NodeId::generate()];
        let coordinator = NodeId::generate();

        let plan = RecoveryPlan::new(
            target_nodes.clone(),
            RecoveryStrategy::Fast,
            coordinator,
        );

        assert_eq!(plan.target_nodes, target_nodes);
        assert_eq!(plan.strategy, RecoveryStrategy::Fast);
        assert_eq!(plan.coordinator, coordinator);
        assert!(!plan.steps.is_empty());
        assert!(plan.expected_duration > Duration::from_millis(0));
    }

    #[test]
    fn test_recovery_strategies() {
        assert_eq!(RecoveryStrategy::default(), RecoveryStrategy::Conservative);
        assert_ne!(RecoveryStrategy::Fast, RecoveryStrategy::Conservative);
    }

    #[tokio::test]
    async fn test_recovery_manager_creation() {
        let manager_id = NodeId::generate();
        let timeout = Duration::from_secs(30);
        let manager = RecoveryManager::new(manager_id, timeout);

        assert_eq!(manager.manager_id, manager_id);
        assert_eq!(manager.validation_timeout, timeout);
    }

    #[tokio::test]
    async fn test_recovery_plan_validation() {
        let manager_id = NodeId::generate();
        let manager = RecoveryManager::new(manager_id, Duration::from_secs(30));

        // Empty nodes should fail
        let result = manager.create_recovery_plan(vec![], RecoveryStrategy::Fast).await;
        assert!(result.is_err());

        // Valid nodes should succeed
        let nodes = vec![NodeId::generate()];
        let result = manager.create_recovery_plan(nodes, RecoveryStrategy::Fast).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_recovery_step_result() {
        let step = RecoveryStep::ValidateClusterState;
        let result = RecoveryStepResult {
            step: step.clone(),
            success: true,
            duration: Duration::from_millis(100),
            data_processed: 1024,
            error: None,
            timestamp: SystemTime::now(),
        };

        assert_eq!(result.step, step);
        assert!(result.success);
        assert_eq!(result.data_processed, 1024);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_recovery_stats() {
        let manager = RecoveryManager::new(NodeId::generate(), Duration::from_secs(30));
        let stats = manager.get_stats().await;

        assert_eq!(stats.total_recoveries, 0);
        assert_eq!(stats.successful_recoveries, 0);
        assert_eq!(stats.failed_recoveries, 0);
        assert_eq!(stats.total_data_recovered, 0);
    }
}