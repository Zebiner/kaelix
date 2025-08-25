//! # Replica Recovery Manager
//!
//! Handles replica failure detection, recovery strategies, and automatic failover
//! to maintain replication availability and data consistency.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use kaelix_cluster::types::NodeId;

use super::{
    ReplicationConfig, Result,
};

/// Recovery strategies for different failure scenarios
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Immediate recovery with full catch-up
    Immediate,
    /// Gradual recovery with rate limiting
    Gradual,
    /// Manual recovery requiring operator intervention
    Manual,
    /// Replace failed replica with new one
    Replace,
}

/// Types of replica failures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureType {
    /// Network connectivity issues
    NetworkFailure,
    /// Node completely down
    NodeFailure,
    /// Slow/degraded performance
    PerformanceDegradation,
    /// Data corruption detected
    DataCorruption,
    /// Replica lagging too far behind
    ExcessiveLag,
}

/// Recovery task tracking
#[derive(Debug, Clone)]
pub struct RecoveryTask {
    /// Task identifier
    pub task_id: u64,
    /// Target replica for recovery
    pub replica_id: NodeId,
    /// Type of failure being recovered
    pub failure_type: FailureType,
    /// Recovery strategy being used
    pub strategy: RecoveryStrategy,
    /// When recovery started
    pub started_at: Instant,
    /// Current progress (0.0 to 1.0)
    pub progress: f64,
    /// Last activity timestamp
    pub last_activity: Instant,
    /// Error count during recovery
    pub error_count: usize,
    /// Maximum errors before giving up
    pub max_errors: usize,
}

/// Recovery statistics and metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStatistics {
    /// Total recovery attempts
    pub total_recoveries: u64,
    /// Successful recoveries
    pub successful_recoveries: u64,
    /// Failed recoveries
    pub failed_recoveries: u64,
    /// Active recovery tasks
    pub active_recoveries: usize,
    /// Average recovery time
    pub avg_recovery_time: Duration,
    /// Recovery success rate
    pub success_rate: f64,
}

/// Recovery manager for handling replica failures
#[derive(Clone)]
pub struct RecoveryManager {
    /// Local node identifier
    local_node_id: NodeId,
    
    /// Recovery configuration
    config: ReplicationConfig,
    
    /// Active recovery tasks
    active_tasks: Arc<RwLock<HashMap<u64, RecoveryTask>>>,
    
    /// Recovery history for analysis
    recovery_history: Arc<RwLock<VecDeque<CompletedRecovery>>>,
    
    /// Next task ID generator
    next_task_id: Arc<std::sync::atomic::AtomicU64>,
    
    /// Failure detection thresholds
    failure_thresholds: FailureThresholds,
    
    /// Recovery statistics
    statistics: Arc<RwLock<RecoveryStatistics>>,
}

/// Thresholds for detecting different types of failures
#[derive(Debug, Clone)]
struct FailureThresholds {
    /// Maximum heartbeat miss count before network failure
    max_heartbeat_misses: u32,
    /// Maximum lag before excessive lag failure
    max_lag_offset: u64,
    /// Maximum response time before performance degradation
    max_response_time: Duration,
    /// Health score threshold below which replica is unhealthy
    min_health_score: f64,
}

impl Default for FailureThresholds {
    fn default() -> Self {
        Self {
            max_heartbeat_misses: 3,
            max_lag_offset: 1000,
            max_response_time: Duration::from_secs(5),
            min_health_score: 0.3,
        }
    }
}

/// Completed recovery record for analysis
#[derive(Debug, Clone)]
struct CompletedRecovery {
    task_id: u64,
    replica_id: NodeId,
    failure_type: FailureType,
    strategy: RecoveryStrategy,
    started_at: Instant,
    completed_at: Instant,
    success: bool,
    entries_recovered: u64,
    error_count: usize,
}

impl RecoveryManager {
    /// Create a new recovery manager
    pub fn new(local_node_id: NodeId, config: ReplicationConfig) -> Self {
        Self {
            local_node_id,
            config,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            recovery_history: Arc::new(RwLock::new(VecDeque::new())),
            next_task_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            failure_thresholds: FailureThresholds::default(),
            statistics: Arc::new(RwLock::new(RecoveryStatistics {
                total_recoveries: 0,
                successful_recoveries: 0,
                failed_recoveries: 0,
                active_recoveries: 0,
                avg_recovery_time: Duration::from_secs(0),
                success_rate: 0.0,
            })),
        }
    }

    /// Detect failure type based on replica state
    pub async fn detect_failure_type(
        &self,
        replica_id: NodeId,
        last_heartbeat: Instant,
        lag_offset: u64,
        health_score: f64,
        response_time: Duration,
    ) -> Option<FailureType> {
        debug!("Analyzing replica {} for potential failures", replica_id);

        // Check for network failure (no heartbeats)
        if last_heartbeat.elapsed() > self.config.heartbeat_interval * 3 {
            return Some(FailureType::NetworkFailure);
        }

        // Check for excessive lag
        if lag_offset > self.failure_thresholds.max_lag_offset {
            return Some(FailureType::ExcessiveLag);
        }

        // Check for performance degradation
        if response_time > self.failure_thresholds.max_response_time {
            return Some(FailureType::PerformanceDegradation);
        }

        // Check for general health issues
        if health_score < self.failure_thresholds.min_health_score {
            return Some(FailureType::NodeFailure);
        }

        None
    }

    /// Start recovery for a failed replica
    pub async fn start_recovery(
        &mut self,
        replica_id: NodeId,
        failure_type: FailureType,
    ) -> Result<u64> {
        info!("Starting recovery for replica {} (failure: {:?})", replica_id, failure_type);

        // Check if recovery is already in progress
        {
            let active_tasks = self.active_tasks.read().await;
            for task in active_tasks.values() {
                if task.replica_id == replica_id {
                    warn!("Recovery already in progress for replica {}", replica_id);
                    return Ok(task.task_id);
                }
            }
        }

        let task_id = self.next_task_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let strategy = self.select_recovery_strategy(failure_type);

        let recovery_task = RecoveryTask {
            task_id,
            replica_id,
            failure_type,
            strategy,
            started_at: Instant::now(),
            progress: 0.0,
            last_activity: Instant::now(),
            error_count: 0,
            max_errors: 5,
        };

        // Add to active tasks
        {
            let mut active_tasks = self.active_tasks.write().await;
            active_tasks.insert(task_id, recovery_task.clone());
        }

        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.total_recoveries += 1;
            stats.active_recoveries += 1;
        }

        // Start recovery process asynchronously
        self.execute_recovery(recovery_task).await?;

        Ok(task_id)
    }

    /// Get status of a specific recovery task
    pub async fn get_recovery_status(&self, task_id: u64) -> Option<RecoveryTask> {
        let active_tasks = self.active_tasks.read().await;
        active_tasks.get(&task_id).cloned()
    }

    /// Get all active recovery tasks
    pub async fn get_active_recoveries(&self) -> Vec<RecoveryTask> {
        let active_tasks = self.active_tasks.read().await;
        active_tasks.values().cloned().collect()
    }

    /// Cancel a recovery task
    pub async fn cancel_recovery(&mut self, task_id: u64) -> Result<()> {
        info!("Cancelling recovery task {}", task_id);

        let mut active_tasks = self.active_tasks.write().await;
        if let Some(task) = active_tasks.remove(&task_id) {
            // Record as failed recovery
            let completed = CompletedRecovery {
                task_id: task.task_id,
                replica_id: task.replica_id,
                failure_type: task.failure_type,
                strategy: task.strategy,
                started_at: task.started_at,
                completed_at: Instant::now(),
                success: false,
                entries_recovered: 0,
                error_count: task.error_count,
            };

            self.record_completed_recovery(completed).await;
            
            // Update statistics
            let mut stats = self.statistics.write().await;
            stats.active_recoveries = stats.active_recoveries.saturating_sub(1);
            stats.failed_recoveries += 1;
            self.update_success_rate(&mut stats);

            info!("Recovery task {} cancelled", task_id);
        } else {
            warn!("Recovery task {} not found for cancellation", task_id);
        }

        Ok(())
    }

    /// Get recovery statistics
    pub async fn get_statistics(&self) -> RecoveryStatistics {
        let stats = self.statistics.read().await;
        stats.clone()
    }

    /// Clean up completed or stale recovery tasks
    pub async fn cleanup_stale_tasks(&mut self) -> Result<usize> {
        let mut removed_count = 0;
        let stale_threshold = Duration::from_secs(3600); // 1 hour

        let mut active_tasks = self.active_tasks.write().await;
        let task_ids: Vec<u64> = active_tasks.keys().copied().collect();

        for task_id in task_ids {
            if let Some(task) = active_tasks.get(&task_id) {
                if task.last_activity.elapsed() > stale_threshold {
                    warn!("Removing stale recovery task {} for replica {}", task_id, task.replica_id);
                    active_tasks.remove(&task_id);
                    removed_count += 1;
                }
            }
        }

        if removed_count > 0 {
            info!("Cleaned up {} stale recovery tasks", removed_count);
        }

        Ok(removed_count)
    }

    // ========================================
    // Private Implementation Methods
    // ========================================

    /// Select appropriate recovery strategy based on failure type
    fn select_recovery_strategy(&self, failure_type: FailureType) -> RecoveryStrategy {
        match failure_type {
            FailureType::NetworkFailure => RecoveryStrategy::Immediate,
            FailureType::ExcessiveLag => RecoveryStrategy::Gradual,
            FailureType::PerformanceDegradation => RecoveryStrategy::Gradual,
            FailureType::NodeFailure => RecoveryStrategy::Replace,
            FailureType::DataCorruption => RecoveryStrategy::Manual,
        }
    }

    /// Execute the recovery process
    async fn execute_recovery(&self, mut task: RecoveryTask) -> Result<()> {
        let task_id = task.task_id;
        let replica_id = task.replica_id;
        
        info!("Executing recovery task {} for replica {} using strategy {:?}",
            task_id, replica_id, task.strategy);

        let result = match task.strategy {
            RecoveryStrategy::Immediate => self.immediate_recovery(&mut task).await,
            RecoveryStrategy::Gradual => self.gradual_recovery(&mut task).await,
            RecoveryStrategy::Manual => self.manual_recovery(&mut task).await,
            RecoveryStrategy::Replace => self.replace_recovery(&mut task).await,
        };

        // Complete the task
        self.complete_recovery_task(task, result.is_ok()).await;

        result
    }

    /// Immediate full recovery strategy
    async fn immediate_recovery(&self, task: &mut RecoveryTask) -> Result<()> {
        info!("Starting immediate recovery for replica {}", task.replica_id);
        
        // Simulate recovery process
        let total_steps = 10;
        for step in 1..=total_steps {
            // Simulate some work
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Update progress
            task.progress = step as f64 / total_steps as f64;
            task.last_activity = Instant::now();
            
            // Update task in storage
            self.update_active_task(task.clone()).await;
            
            debug!("Recovery progress for {}: {:.1}%", task.replica_id, task.progress * 100.0);
        }
        
        info!("Immediate recovery completed for replica {}", task.replica_id);
        Ok(())
    }

    /// Gradual recovery with rate limiting
    async fn gradual_recovery(&self, task: &mut RecoveryTask) -> Result<()> {
        info!("Starting gradual recovery for replica {}", task.replica_id);
        
        // Slower recovery with longer delays
        let total_steps = 20;
        for step in 1..=total_steps {
            // Longer delay for gradual recovery
            tokio::time::sleep(Duration::from_millis(200)).await;
            
            task.progress = step as f64 / total_steps as f64;
            task.last_activity = Instant::now();
            
            self.update_active_task(task.clone()).await;
            
            if step % 5 == 0 {
                debug!("Gradual recovery progress for {}: {:.1}%", 
                    task.replica_id, task.progress * 100.0);
            }
        }
        
        info!("Gradual recovery completed for replica {}", task.replica_id);
        Ok(())
    }

    /// Manual recovery requiring operator intervention
    async fn manual_recovery(&self, task: &mut RecoveryTask) -> Result<()> {
        warn!("Manual recovery required for replica {} - operator intervention needed", 
            task.replica_id);
        
        // Set progress to indicate manual intervention required
        task.progress = 0.0;
        task.last_activity = Instant::now();
        self.update_active_task(task.clone()).await;
        
        // For now, just wait and mark as requiring manual intervention
        // In a real system, this would notify operators and wait for resolution
        
        Ok(())
    }

    /// Replace failed replica with a new one
    async fn replace_recovery(&self, task: &mut RecoveryTask) -> Result<()> {
        info!("Starting replica replacement for {}", task.replica_id);
        
        // Simulate finding and initializing a replacement replica
        let total_steps = 15;
        for step in 1..=total_steps {
            tokio::time::sleep(Duration::from_millis(150)).await;
            
            task.progress = step as f64 / total_steps as f64;
            task.last_activity = Instant::now();
            
            self.update_active_task(task.clone()).await;
            
            if step % 3 == 0 {
                debug!("Replacement progress for {}: {:.1}%", 
                    task.replica_id, task.progress * 100.0);
            }
        }
        
        info!("Replica replacement completed for {}", task.replica_id);
        Ok(())
    }

    /// Update an active task in storage
    async fn update_active_task(&self, task: RecoveryTask) {
        let mut active_tasks = self.active_tasks.write().await;
        active_tasks.insert(task.task_id, task);
    }

    /// Complete a recovery task
    async fn complete_recovery_task(&self, task: RecoveryTask, success: bool) {
        info!("Completing recovery task {} for replica {} (success: {})",
            task.task_id, task.replica_id, success);

        // Remove from active tasks
        {
            let mut active_tasks = self.active_tasks.write().await;
            active_tasks.remove(&task.task_id);
        }

        // Record completion
        let completed = CompletedRecovery {
            task_id: task.task_id,
            replica_id: task.replica_id,
            failure_type: task.failure_type,
            strategy: task.strategy,
            started_at: task.started_at,
            completed_at: Instant::now(),
            success,
            entries_recovered: if success { 1000 } else { 0 }, // Simulated
            error_count: task.error_count,
        };

        self.record_completed_recovery(completed).await;

        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.active_recoveries = stats.active_recoveries.saturating_sub(1);
            
            if success {
                stats.successful_recoveries += 1;
            } else {
                stats.failed_recoveries += 1;
            }
            
            self.update_success_rate(&mut stats);
            self.update_avg_recovery_time(&mut stats, task.started_at.elapsed());
        }
    }

    /// Record a completed recovery for analysis
    async fn record_completed_recovery(&self, completed: CompletedRecovery) {
        let mut history = self.recovery_history.write().await;
        
        // Limit history size
        if history.len() >= 1000 {
            history.pop_front();
        }
        
        history.push_back(completed);
    }

    /// Update success rate in statistics
    fn update_success_rate(&self, stats: &mut RecoveryStatistics) {
        let total = stats.successful_recoveries + stats.failed_recoveries;
        if total > 0 {
            stats.success_rate = stats.successful_recoveries as f64 / total as f64;
        }
    }

    /// Update average recovery time
    fn update_avg_recovery_time(&self, stats: &mut RecoveryStatistics, recovery_time: Duration) {
        // Simple moving average
        let completed = stats.successful_recoveries + stats.failed_recoveries;
        if completed == 1 {
            stats.avg_recovery_time = recovery_time;
        } else {
            let alpha = 0.1; // Smoothing factor
            let old_avg_nanos = stats.avg_recovery_time.as_nanos() as f64;
            let new_time_nanos = recovery_time.as_nanos() as f64;
            let new_avg_nanos = alpha * new_time_nanos + (1.0 - alpha) * old_avg_nanos;
            stats.avg_recovery_time = Duration::from_nanos(new_avg_nanos as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> RecoveryManager {
        let node_id = NodeId::generate();
        let config = ReplicationConfig::default();
        RecoveryManager::new(node_id, config)
    }

    #[tokio::test]
    async fn test_recovery_manager_creation() {
        let manager = create_test_manager();
        let stats = manager.get_statistics().await;
        
        assert_eq!(stats.total_recoveries, 0);
        assert_eq!(stats.active_recoveries, 0);
        assert_eq!(stats.success_rate, 0.0);
    }

    #[tokio::test]
    async fn test_failure_detection() {
        let manager = create_test_manager();
        let replica_id = NodeId::generate();
        
        // Test network failure detection
        let failure = manager.detect_failure_type(
            replica_id,
            Instant::now() - Duration::from_secs(30), // Old heartbeat
            0,
            1.0,
            Duration::from_millis(100),
        ).await;
        
        assert_eq!(failure, Some(FailureType::NetworkFailure));
        
        // Test excessive lag detection
        let failure = manager.detect_failure_type(
            replica_id,
            Instant::now(),
            2000, // High lag
            1.0,
            Duration::from_millis(100),
        ).await;
        
        assert_eq!(failure, Some(FailureType::ExcessiveLag));
    }

    #[tokio::test]
    async fn test_recovery_start_and_status() {
        let mut manager = create_test_manager();
        let replica_id = NodeId::generate();
        
        let task_id = manager.start_recovery(replica_id, FailureType::NetworkFailure)
            .await.unwrap();
        
        assert!(task_id > 0);
        
        let task = manager.get_recovery_status(task_id).await;
        assert!(task.is_some());
        
        let task = task.unwrap();
        assert_eq!(task.replica_id, replica_id);
        assert_eq!(task.failure_type, FailureType::NetworkFailure);
    }

    #[tokio::test]
    async fn test_recovery_cancellation() {
        let mut manager = create_test_manager();
        let replica_id = NodeId::generate();
        
        let task_id = manager.start_recovery(replica_id, FailureType::NetworkFailure)
            .await.unwrap();
        
        // Cancel the recovery
        manager.cancel_recovery(task_id).await.unwrap();
        
        // Task should no longer be active
        let task = manager.get_recovery_status(task_id).await;
        assert!(task.is_none());
        
        // Statistics should reflect the cancellation
        let stats = manager.get_statistics().await;
        assert_eq!(stats.failed_recoveries, 1);
        assert_eq!(stats.active_recoveries, 0);
    }

    #[tokio::test]
    async fn test_recovery_strategy_selection() {
        let manager = create_test_manager();
        
        assert_eq!(manager.select_recovery_strategy(FailureType::NetworkFailure), 
                   RecoveryStrategy::Immediate);
        assert_eq!(manager.select_recovery_strategy(FailureType::ExcessiveLag), 
                   RecoveryStrategy::Gradual);
        assert_eq!(manager.select_recovery_strategy(FailureType::NodeFailure), 
                   RecoveryStrategy::Replace);
        assert_eq!(manager.select_recovery_strategy(FailureType::DataCorruption), 
                   RecoveryStrategy::Manual);
    }

    #[test]
    fn test_failure_thresholds_default() {
        let thresholds = FailureThresholds::default();
        
        assert_eq!(thresholds.max_heartbeat_misses, 3);
        assert_eq!(thresholds.max_lag_offset, 1000);
        assert_eq!(thresholds.min_health_score, 0.3);
    }
}