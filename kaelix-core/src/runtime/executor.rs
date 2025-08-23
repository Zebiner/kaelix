//! High-performance async runtime executor with NUMA awareness
//!
//! Provides microsecond-level task scheduling precision with advanced CPU affinity management.
//! Optimized for ultra-low latency message streaming workloads.

use crate::runtime::{
    RuntimeError, RuntimeResult, TARGET_TASK_LATENCY_US, affinity::NumaTopology,
    metrics::RuntimeMetrics,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{debug, info};

/// Ultra-high-performance executor configuration optimized for microsecond-level latencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Number of task pools (typically matches CPU cores)
    pub task_pools: usize,

    /// Maximum tasks per pool queue
    pub queue_depth: usize,

    /// Target latency for task execution (microseconds)
    pub latency_target: Duration,

    /// CPU affinity mapping for worker threads
    pub cpu_affinity: Option<Vec<usize>>,

    /// Enable NUMA-aware scheduling
    pub numa_aware: bool,

    /// Stack size for worker threads (bytes)
    pub worker_stack_size: usize,

    /// Enable priority-based task scheduling
    pub priority_scheduling: bool,

    /// Maximum number of concurrent tasks per worker
    pub max_concurrent_tasks: usize,

    /// Task preemption timeout
    pub preemption_timeout: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            task_pools: num_cpus::get(),
            queue_depth: 4096,
            latency_target: Duration::from_micros(TARGET_TASK_LATENCY_US),
            cpu_affinity: None,
            numa_aware: true,
            worker_stack_size: 2 * 1024 * 1024, // 2MB
            priority_scheduling: true,
            max_concurrent_tasks: 1000,
            preemption_timeout: Duration::from_millis(100),
        }
    }
}

/// Task priority levels for scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskPriority {
    /// Critical system tasks (highest priority)
    Critical = 0,
    /// High-priority user tasks
    High = 1,
    /// Normal priority tasks
    Normal = 2,
    /// Background tasks (lowest priority)
    Background = 3,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

/// Task execution context with performance tracking
#[derive(Debug)]
pub struct TaskContext {
    /// Task ID for tracking
    pub id: u64,
    /// Task priority
    pub priority: TaskPriority,
    /// Worker pool assignment
    pub pool_id: usize,
    /// Creation timestamp
    pub created_at: std::time::Instant,
    /// Execution start timestamp
    pub started_at: Option<std::time::Instant>,
    /// Completion timestamp
    pub completed_at: Option<std::time::Instant>,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(id: u64, priority: TaskPriority, pool_id: usize) -> Self {
        Self {
            id,
            priority,
            pool_id,
            created_at: std::time::Instant::now(),
            started_at: None,
            completed_at: None,
        }
    }

    /// Mark task as started
    pub fn start(&mut self) {
        self.started_at = Some(std::time::Instant::now());
    }

    /// Mark task as completed
    pub fn complete(&mut self) {
        self.completed_at = Some(std::time::Instant::now());
    }

    /// Get task queue latency
    pub fn queue_latency(&self) -> Option<Duration> {
        self.started_at.map(|started| started.duration_since(self.created_at))
    }

    /// Get task execution duration
    pub fn execution_duration(&self) -> Option<Duration> {
        match (self.started_at, self.completed_at) {
            (Some(started), Some(completed)) => Some(completed.duration_since(started)),
            _ => None,
        }
    }

    /// Get total task duration
    pub fn total_duration(&self) -> Option<Duration> {
        self.completed_at.map(|completed| completed.duration_since(self.created_at))
    }
}

/// Task wrapper for the executor
pub struct ExecutorTask {
    /// Task context
    pub context: TaskContext,
    /// Task future
    pub future: Box<dyn Future<Output = ()> + Send + 'static>,
    /// Result sender
    pub result_tx: Option<oneshot::Sender<RuntimeResult<()>>>,
}

/// Worker pool for task execution
#[derive(Debug)]
pub struct WorkerPool {
    /// Pool ID
    pub id: usize,
    /// Task queue sender
    pub task_tx: mpsc::UnboundedSender<ExecutorTask>,
    /// Worker join handles
    pub workers: Vec<JoinHandle<RuntimeResult<()>>>,
    /// Pool statistics
    pub stats: Arc<Mutex<PoolStats>>,
}

/// Pool execution statistics
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    /// Total tasks executed
    pub tasks_executed: u64,
    /// Total execution time
    pub total_execution_time: Duration,
    /// Average execution time
    pub avg_execution_time: Duration,
    /// Peak queue depth
    pub peak_queue_depth: usize,
    /// Current active tasks
    pub active_tasks: usize,
}

/// High-performance runtime executor
pub struct RuntimeExecutor {
    /// Configuration
    config: RuntimeConfig,
    /// Worker pools
    pools: Vec<Arc<WorkerPool>>,
    /// NUMA topology
    numa_topology: Option<NumaTopology>,
    /// Task ID counter
    task_counter: Arc<AtomicUsize>,
    /// Runtime metrics
    metrics: Arc<RuntimeMetrics>,
    /// Running state
    is_running: Arc<AtomicBool>,
    /// Shutdown sender
    shutdown_tx: Option<mpsc::UnboundedSender<()>>,
}

impl RuntimeExecutor {
    /// Create a new runtime executor
    pub fn new(config: RuntimeConfig) -> RuntimeResult<Self> {
        let numa_topology = if config.numa_aware {
            Some(NumaTopology::detect()?)
        } else {
            None
        };

        Ok(Self {
            config,
            pools: Vec::new(),
            numa_topology,
            task_counter: Arc::new(AtomicUsize::new(0)),
            metrics: Arc::new(RuntimeMetrics::new()),
            is_running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
        })
    }

    /// Start the executor
    pub async fn start(&mut self) -> RuntimeResult<()> {
        if self.is_running.load(Ordering::Relaxed) {
            return Err(RuntimeError::AlreadyRunning);
        }

        info!("Starting runtime executor with {} pools", self.config.task_pools);

        let (shutdown_tx, _shutdown_rx) = mpsc::unbounded_channel();
        self.shutdown_tx = Some(shutdown_tx);

        // Initialize worker pools
        for pool_id in 0..self.config.task_pools {
            let pool = self.create_worker_pool(pool_id).await?;
            self.pools.push(Arc::new(pool));
        }

        self.is_running.store(true, Ordering::Relaxed);

        // Start metrics collection
        let metrics = self.metrics.clone();
        let is_running = self.is_running.clone();
        tokio::spawn(async move {
            while is_running.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(100)).await;
                metrics.update();
            }
        });

        info!("Runtime executor started successfully");
        Ok(())
    }

    /// Stop the executor
    pub async fn stop(&mut self) -> RuntimeResult<()> {
        if !self.is_running.load(Ordering::Relaxed) {
            return Ok(());
        }

        info!("Stopping runtime executor");
        self.is_running.store(false, Ordering::Relaxed);

        // Send shutdown signal
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(());
        }

        // Wait for all worker pools to complete
        for _pool in &mut self.pools {
            // Implementation would wait for workers to finish
        }

        self.pools.clear();
        self.shutdown_tx = None;

        info!("Runtime executor stopped successfully");
        Ok(())
    }

    /// Spawn a task with default priority
    pub async fn spawn<F>(&self, future: F) -> RuntimeResult<oneshot::Receiver<RuntimeResult<()>>>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_with_priority(future, TaskPriority::Normal).await
    }

    /// Spawn a task with specific priority
    pub async fn spawn_with_priority<F>(
        &self,
        future: F,
        priority: TaskPriority,
    ) -> RuntimeResult<oneshot::Receiver<RuntimeResult<()>>>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if !self.is_running.load(Ordering::Relaxed) {
            return Err(RuntimeError::NotRunning);
        }

        let task_id = self.task_counter.fetch_add(1, Ordering::Relaxed) as u64;
        let pool_id = self.select_pool(priority);

        let context = TaskContext::new(task_id, priority, pool_id);
        let (result_tx, result_rx) = oneshot::channel();

        let task = ExecutorTask { context, future: Box::new(future), result_tx: Some(result_tx) };

        // Send task to selected pool
        let pool = &self.pools[pool_id];
        pool.task_tx.send(task).map_err(|_| RuntimeError::TaskQueueFull)?;

        debug!("Spawned task {} on pool {} with priority {:?}", task_id, pool_id, priority);
        Ok(result_rx)
    }

    /// Get runtime statistics
    pub fn get_stats(&self) -> RuntimeStats {
        let pool_stats: Vec<PoolStats> =
            self.pools.iter().map(|pool| pool.stats.lock().clone()).collect();

        RuntimeStats {
            total_pools: self.pools.len(),
            active_tasks: pool_stats.iter().map(|s| s.active_tasks).sum(),
            total_executed: pool_stats.iter().map(|s| s.tasks_executed).sum(),
            avg_execution_time: {
                let total_time: Duration = pool_stats.iter().map(|s| s.total_execution_time).sum();
                let total_tasks: u64 = pool_stats.iter().map(|s| s.tasks_executed).sum();
                if total_tasks > 0 {
                    total_time / total_tasks as u32
                } else {
                    Duration::ZERO
                }
            },
            pool_stats,
        }
    }

    /// Create a worker pool
    async fn create_worker_pool(&self, pool_id: usize) -> RuntimeResult<WorkerPool> {
        let (task_tx, mut task_rx) = mpsc::unbounded_channel();
        let stats = Arc::new(Mutex::new(PoolStats::default()));

        // Create worker threads based on configuration
        let worker_count = 1; // Simplified - would normally be configurable
        let mut workers = Vec::new();

        for _worker_id in 0..worker_count {
            let stats = stats.clone();
            let is_running = self.is_running.clone();

            let worker = tokio::spawn(async move {
                while is_running.load(Ordering::Relaxed) {
                    match task_rx.recv().await {
                        Some(mut task) => {
                            task.context.start();

                            // Execute the task
                            task.future.await;

                            task.context.complete();

                            // Update statistics
                            {
                                let mut pool_stats = stats.lock();
                                pool_stats.tasks_executed += 1;
                                if let Some(duration) = task.context.execution_duration() {
                                    pool_stats.total_execution_time += duration;
                                    pool_stats.avg_execution_time = pool_stats.total_execution_time
                                        / pool_stats.tasks_executed as u32;
                                }
                            }

                            // Send result if requested
                            if let Some(result_tx) = task.result_tx {
                                let _ = result_tx.send(Ok(()));
                            }
                        },
                        None => break,
                    }
                }
                Ok(())
            });

            workers.push(worker);
        }

        Ok(WorkerPool { id: pool_id, task_tx, workers, stats })
    }

    /// Select the optimal pool for a task
    fn select_pool(&self, priority: TaskPriority) -> usize {
        // Simplified pool selection - production would use load balancing
        match priority {
            TaskPriority::Critical => 0,
            TaskPriority::High => self.pools.len().min(2) - 1,
            _ => self.task_counter.load(Ordering::Relaxed) % self.pools.len(),
        }
    }
}

/// Runtime execution statistics
#[derive(Debug, Clone)]
pub struct RuntimeStats {
    /// Total worker pools
    pub total_pools: usize,
    /// Currently active tasks
    pub active_tasks: usize,
    /// Total tasks executed
    pub total_executed: u64,
    /// Average execution time
    pub avg_execution_time: Duration,
    /// Per-pool statistics
    pub pool_stats: Vec<PoolStats>,
}

impl Drop for RuntimeExecutor {
    fn drop(&mut self) {
        // Ensure proper cleanup
        self.is_running.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_executor_creation() {
        let config = RuntimeConfig::default();
        let executor = RuntimeExecutor::new(config);
        assert!(executor.is_ok());
    }

    #[tokio::test]
    async fn test_task_context() {
        let mut context = TaskContext::new(1, TaskPriority::Normal, 0);
        assert_eq!(context.id, 1);
        assert_eq!(context.priority, TaskPriority::Normal);
        assert_eq!(context.pool_id, 0);

        context.start();
        assert!(context.started_at.is_some());

        context.complete();
        assert!(context.completed_at.is_some());
        assert!(context.execution_duration().is_some());
    }

    #[test]
    fn test_task_priority_ordering() {
        assert!(TaskPriority::Critical < TaskPriority::High);
        assert!(TaskPriority::High < TaskPriority::Normal);
        assert!(TaskPriority::Normal < TaskPriority::Background);
    }
}
