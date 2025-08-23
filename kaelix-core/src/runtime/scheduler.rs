use crate::config::hot_reload::HotReloadConfig;
use crate::multiplexing::Priority;
use crate::runtime::affinity::{NodeId, NumaTopology};
use crate::runtime::worker::WorkerId;
use crate::telemetry::performance::PerformanceMetric;
use crossbeam::queue::SegQueue;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Custom Result type for scheduler
pub type Result<T> = std::result::Result<T, SchedulerError>;

/// Scheduler-specific error types
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Scheduler already running")]
    AlreadyRunning,
    #[error("Scheduler not running")]
    NotRunning,
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Task execution error: {0}")]
    TaskExecution(String),
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
}

/// Task execution error
#[derive(Debug, thiserror::Error)]
pub enum TaskExecutionError {
    #[error("Task timeout")]
    Timeout,
    #[error("Task cancelled")]
    Cancelled,
    #[error("Task failed: {0}")]
    Failed(String),
}

/// Task types for scheduling
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TaskType {
    /// Message processing task
    MessageProcess,
    /// Stream management task
    StreamManagement,
    /// Telemetry collection task
    TelemetryCollection,
    /// Plugin execution task
    PluginExecution,
    /// Periodic maintenance task
    MaintenanceTask,
    /// Hot reload task
    HotReload,
}

/// Priority-aware task wrapper
#[derive(Debug)]
pub struct ScheduledTask {
    pub id: TaskId,
    pub task_type: TaskType,
    pub priority: Priority,
    pub submitted_at: Instant,
    pub estimated_duration: Option<Duration>,
    pub numa_preference: Option<NodeId>,
    pub handle: JoinHandle<std::result::Result<(), TaskExecutionError>>,
}

/// Unique task identifier
pub type TaskId = u64;

/// Queue statistics for monitoring
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub total_pending: usize,
    pub high_priority_pending: usize,
    pub medium_priority_pending: usize,
    pub low_priority_pending: usize,
    pub average_wait_time: Duration,
    pub throughput: u64,
}

/// Task queues organized by priority
#[derive(Debug)]
pub struct TaskQueues {
    high_priority: SegQueue<ScheduledTask>,
    medium_priority: SegQueue<ScheduledTask>,
    low_priority: SegQueue<ScheduledTask>,
    stats: Arc<RwLock<QueueStats>>,
}

impl TaskQueues {
    pub fn new() -> Self {
        Self {
            high_priority: SegQueue::new(),
            medium_priority: SegQueue::new(),
            low_priority: SegQueue::new(),
            stats: Arc::new(RwLock::new(QueueStats {
                total_pending: 0,
                high_priority_pending: 0,
                medium_priority_pending: 0,
                low_priority_pending: 0,
                average_wait_time: Duration::from_millis(0),
                throughput: 0,
            })),
        }
    }

    pub fn push(&self, task: ScheduledTask) {
        match task.priority {
            Priority::High | Priority::Critical => {
                self.high_priority.push(task);
                self.stats.write().high_priority_pending += 1;
            },
            Priority::Normal => {
                self.medium_priority.push(task);
                self.stats.write().medium_priority_pending += 1;
            },
            Priority::Low => {
                self.low_priority.push(task);
                self.stats.write().low_priority_pending += 1;
            },
        }
        self.stats.write().total_pending += 1;
    }

    pub fn pop(&self) -> Option<ScheduledTask> {
        // Try high priority first
        if let Some(task) = self.high_priority.pop() {
            self.stats.write().high_priority_pending =
                self.stats.read().high_priority_pending.saturating_sub(1);
            self.stats.write().total_pending = self.stats.read().total_pending.saturating_sub(1);
            return Some(task);
        }

        // Then medium priority
        if let Some(task) = self.medium_priority.pop() {
            self.stats.write().medium_priority_pending =
                self.stats.read().medium_priority_pending.saturating_sub(1);
            self.stats.write().total_pending = self.stats.read().total_pending.saturating_sub(1);
            return Some(task);
        }

        // Finally low priority
        if let Some(task) = self.low_priority.pop() {
            self.stats.write().low_priority_pending =
                self.stats.read().low_priority_pending.saturating_sub(1);
            self.stats.write().total_pending = self.stats.read().total_pending.saturating_sub(1);
            return Some(task);
        }

        None
    }

    pub fn is_empty(&self) -> bool {
        self.high_priority.is_empty()
            && self.medium_priority.is_empty()
            && self.low_priority.is_empty()
    }

    pub fn stats(&self) -> QueueStats {
        self.stats.read().clone()
    }
}

/// Advanced scheduler with NUMA awareness and hot reloading
#[derive(Debug)]
pub struct AdvancedScheduler {
    /// Task queues organized by priority
    queues: Arc<TaskQueues>,
    /// Worker assignments to NUMA nodes
    worker_assignments: Arc<RwLock<HashMap<WorkerId, NodeId>>>,
    /// NUMA topology information
    numa_topology: Arc<NumaTopology>,
    /// Scheduler configuration
    config: Arc<RwLock<SchedulerConfig>>,
    /// Performance metrics
    metrics: Arc<PerformanceMetric>,
    /// Hot reload configuration
    hot_reload_config: Arc<RwLock<HotReloadConfig>>,
    /// Task ID generator
    task_id_counter: AtomicU64,
    /// Scheduler state
    is_running: AtomicBool,
}

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum number of pending tasks per priority level
    pub max_pending_per_priority: usize,
    /// Task timeout duration
    pub task_timeout: Duration,
    /// Load balancing strategy
    pub load_balancing_strategy: LoadBalancingStrategy,
    /// NUMA affinity enabled
    pub numa_affinity_enabled: bool,
    /// Work stealing enabled
    pub work_stealing_enabled: bool,
    /// Metrics collection interval
    pub metrics_interval: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_pending_per_priority: 10000,
            task_timeout: Duration::from_secs(30),
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
            numa_affinity_enabled: true,
            work_stealing_enabled: true,
            metrics_interval: Duration::from_secs(1),
        }
    }
}

/// Load balancing strategies
#[derive(Debug, Clone, PartialEq)]
pub enum LoadBalancingStrategy {
    /// Round-robin assignment
    RoundRobin,
    /// Least loaded worker
    LeastLoaded,
    /// NUMA-aware assignment
    NumaAware,
    /// Weighted round-robin
    WeightedRoundRobin,
}

impl AdvancedScheduler {
    /// Create new advanced scheduler
    pub fn new(
        numa_topology: NumaTopology,
        config: SchedulerConfig,
        hot_reload_config: HotReloadConfig,
    ) -> Result<Self> {
        let scheduler = Self {
            queues: Arc::new(TaskQueues::new()),
            worker_assignments: Arc::new(RwLock::new(HashMap::new())),
            numa_topology: Arc::new(numa_topology),
            config: Arc::new(RwLock::new(config)),
            metrics: Arc::new(PerformanceMetric::new("scheduler".to_string())),
            hot_reload_config: Arc::new(RwLock::new(hot_reload_config)),
            task_id_counter: AtomicU64::new(0),
            is_running: AtomicBool::new(false),
        };

        Ok(scheduler)
    }

    /// Start the scheduler
    pub async fn start(&self) -> Result<()> {
        if self
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(SchedulerError::AlreadyRunning);
        }

        info!("Starting advanced scheduler");

        // Initialize worker assignments based on NUMA topology
        self.initialize_worker_assignments()?;

        // Start metrics collection
        self.start_metrics_collection().await?;

        info!("Advanced scheduler started successfully");
        Ok(())
    }

    /// Stop the scheduler
    pub async fn stop(&self) -> Result<()> {
        if self
            .is_running
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(SchedulerError::NotRunning);
        }

        info!("Stopping advanced scheduler");

        // Wait for pending tasks to complete or timeout
        self.wait_for_pending_tasks().await?;

        info!("Advanced scheduler stopped successfully");
        Ok(())
    }

    /// Schedule a task
    pub async fn schedule_task(
        &self,
        task_type: TaskType,
        priority: Priority,
        estimated_duration: Option<Duration>,
        numa_preference: Option<NodeId>,
        task_fn: impl std::future::Future<Output = std::result::Result<(), TaskExecutionError>>
        + Send
        + 'static,
    ) -> Result<TaskId> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(SchedulerError::NotRunning);
        }

        let task_id = self.task_id_counter.fetch_add(1, Ordering::SeqCst);

        let handle = tokio::spawn(task_fn);

        let scheduled_task = ScheduledTask {
            id: task_id,
            task_type,
            priority,
            submitted_at: Instant::now(),
            estimated_duration,
            numa_preference,
            handle,
        };

        // Select optimal worker based on strategy
        let _selected_worker = self.select_worker(&scheduled_task)?;

        // Add to appropriate queue
        self.queues.push(scheduled_task);

        // Update metrics
        self.metrics.increment("tasks_scheduled".to_string());

        Ok(task_id)
    }

    /// Get next task for worker
    pub fn get_next_task(&self, worker_id: WorkerId) -> Option<ScheduledTask> {
        // Check worker-specific queue first if NUMA-aware
        if self.config.read().numa_affinity_enabled {
            if let Some(task) = self.get_numa_preferred_task(worker_id) {
                return Some(task);
            }
        }

        // Fall back to general queue
        self.queues.pop()
    }

    /// Initialize worker assignments to NUMA nodes
    fn initialize_worker_assignments(&self) -> Result<()> {
        let mut assignments = self.worker_assignments.write();
        let nodes = self.numa_topology.available_nodes();

        // Simple round-robin assignment for now
        for worker_idx in 0..8 {
            let worker_id = WorkerId::new(worker_idx);
            let node_idx = worker_idx % nodes.len();
            assignments.insert(worker_id, nodes[node_idx]);
        }

        info!("Initialized {} worker assignments", assignments.len());
        Ok(())
    }

    /// Start metrics collection
    async fn start_metrics_collection(&self) -> Result<()> {
        let metrics = Arc::clone(&self.metrics);
        let queues = Arc::clone(&self.queues);
        let interval = self.config.read().metrics_interval;

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                let stats = queues.stats();
                metrics.set_gauge("queue_depth".to_string(), stats.total_pending as f64);
                metrics.set_gauge("throughput".to_string(), stats.throughput as f64);
            }
        });

        Ok(())
    }

    /// Wait for pending tasks to complete
    async fn wait_for_pending_tasks(&self) -> Result<()> {
        let timeout = self.config.read().task_timeout;
        let start = Instant::now();

        while !self.queues.is_empty() && start.elapsed() < timeout {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !self.queues.is_empty() {
            warn!("Timeout waiting for pending tasks to complete");
        }

        Ok(())
    }

    /// Select optimal worker for task
    fn select_worker(&self, task: &ScheduledTask) -> Result<Option<WorkerId>> {
        match self.config.read().load_balancing_strategy {
            LoadBalancingStrategy::RoundRobin => self.select_round_robin_worker(),
            LoadBalancingStrategy::LeastLoaded => self.select_least_loaded_worker(),
            LoadBalancingStrategy::NumaAware => self.select_numa_aware_worker(task),
            LoadBalancingStrategy::WeightedRoundRobin => self.select_weighted_round_robin_worker(),
        }
    }

    fn select_round_robin_worker(&self) -> Result<Option<WorkerId>> {
        // Simple round-robin implementation
        Ok(Some(WorkerId::new(0))) // Would implement proper round-robin
    }

    fn select_least_loaded_worker(&self) -> Result<Option<WorkerId>> {
        // Would implement load tracking and selection
        Ok(Some(WorkerId::new(0))) // Would implement proper load balancing
    }

    fn select_numa_aware_worker(&self, task: &ScheduledTask) -> Result<Option<WorkerId>> {
        if let Some(preferred_node) = task.numa_preference {
            return Ok(self.select_numa_worker(preferred_node));
        }

        // Fall back to round-robin
        self.select_round_robin_worker()
    }

    fn select_weighted_round_robin_worker(&self) -> Result<Option<WorkerId>> {
        // Would implement weighted assignment based on worker capabilities
        Ok(Some(WorkerId::new(0))) // Would implement proper load balancing
    }

    fn select_numa_worker(&self, numa_node: NodeId) -> Option<WorkerId> {
        // Find worker assigned to the specified NUMA node
        let assignments = self.worker_assignments.read();
        assignments
            .iter()
            .find(|&(_worker_id, node)| *node == numa_node)
            .map(|(&worker_id, _node)| worker_id)
    }

    /// Get queue statistics
    pub fn queue_stats(&self) -> QueueStats {
        self.queues.stats()
    }

    /// Get NUMA-preferred task for worker
    fn get_numa_preferred_task(&self, _worker_id: WorkerId) -> Option<ScheduledTask> {
        // Look for tasks with matching NUMA preference
        // This is simplified - would need more sophisticated matching
        self.queues.pop()
    }

    /// Update configuration with hot reload
    pub async fn update_config(&self, new_config: SchedulerConfig) -> Result<()> {
        let mut config = self.config.write();
        *config = new_config;

        info!("Scheduler configuration updated via hot reload");

        // Re-initialize worker assignments if NUMA settings changed
        if config.numa_affinity_enabled {
            drop(config); // Release lock before calling initialize
            self.initialize_worker_assignments()?;
        }

        Ok(())
    }

    /// Get current configuration
    pub fn get_config(&self) -> SchedulerConfig {
        self.config.read().clone()
    }

    /// Health check
    pub fn health_check(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }
}

// Add WorkerId implementation
impl WorkerId {
    pub fn new(id: usize) -> Self {
        Self { id }
    }

    pub fn id(&self) -> usize {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::affinity::NumaTopology;

    #[tokio::test]
    async fn test_scheduler_creation() {
        let numa_topology = NumaTopology::detect().unwrap();
        let config = SchedulerConfig::default();
        let hot_reload_config = HotReloadConfig::default();

        let scheduler = AdvancedScheduler::new(numa_topology, config, hot_reload_config).unwrap();
        assert!(!scheduler.health_check());
    }

    #[tokio::test]
    async fn test_task_scheduling() {
        let numa_topology = NumaTopology::detect().unwrap();
        let config = SchedulerConfig::default();
        let hot_reload_config = HotReloadConfig::default();

        let scheduler = AdvancedScheduler::new(numa_topology, config, hot_reload_config).unwrap();
        scheduler.start().await.unwrap();

        let task_id = scheduler
            .schedule_task(
                TaskType::MessageProcess,
                Priority::High,
                Some(Duration::from_millis(100)),
                None,
                async { Ok(()) },
            )
            .await
            .unwrap();

        assert!(task_id > 0);

        scheduler.stop().await.unwrap();
    }

    #[test]
    fn test_task_queues() {
        let queues = TaskQueues::new();
        assert!(queues.is_empty());

        let stats = queues.stats();
        assert_eq!(stats.total_pending, 0);
    }
}
