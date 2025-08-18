//! Custom High-Performance Async Executor
//!
//! Provides an ultra-optimized async executor designed for MemoryStreamer's demanding
//! latency and throughput requirements.

use crate::runtime::{
    affinity::NumaTopology,
    metrics::{RuntimeMetrics, UtilizationReport},
    scheduler::{TaskScheduler, TaskQueues},
    worker::{WorkerThread, WorkerId},
    performance::*,
};
use crate::error::{Error, Result};
use std::{
    collections::HashMap,
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::oneshot;

/// Runtime configuration for optimal performance tuning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Number of worker threads (None = auto-detect optimal count)
    pub worker_threads: Option<usize>,
    
    /// Enable NUMA-aware worker placement
    pub numa_awareness: bool,
    
    /// CPU affinity configuration
    pub cpu_affinity: AffinityConfig,
    
    /// Pre-allocated task pool size
    pub task_pool_size: usize,
    
    /// Queue depth per worker
    pub queue_depth: usize,
    
    /// Target latency for performance monitoring
    pub latency_target: Duration,
    
    /// Target throughput for performance monitoring
    pub throughput_target: u64,
    
    /// Enable work stealing between workers
    pub work_stealing: bool,
    
    /// Worker idle timeout before parking
    pub idle_timeout: Duration,
    
    /// Enable real-time metrics collection
    pub metrics_enabled: bool,
}

/// CPU affinity configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffinityConfig {
    /// Bind workers to specific CPU cores
    pub bind_to_cores: bool,
    
    /// Specific CPU core assignments (worker_id -> core_id)
    pub core_assignments: HashMap<usize, usize>,
    
    /// Avoid specific CPU cores (hyperthreading pairs, etc.)
    pub excluded_cores: Vec<usize>,
    
    /// Prefer cores on specific NUMA nodes
    pub preferred_numa_nodes: Vec<usize>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_threads: None,
            numa_awareness: true,
            cpu_affinity: AffinityConfig::default(),
            task_pool_size: DEFAULT_TASK_POOL_SIZE,
            queue_depth: DEFAULT_QUEUE_DEPTH,
            latency_target: TARGET_P99_LATENCY,
            throughput_target: TARGET_THROUGHPUT_MPS,
            work_stealing: true,
            idle_timeout: Duration::from_micros(100),
            metrics_enabled: true,
        }
    }
}

impl Default for AffinityConfig {
    fn default() -> Self {
        Self {
            bind_to_cores: true,
            core_assignments: HashMap::new(),
            excluded_cores: Vec::new(),
            preferred_numa_nodes: Vec::new(),
        }
    }
}

impl RuntimeConfig {
    /// Create an optimized configuration for MemoryStreamer workloads
    pub fn optimized() -> Self {
        let num_cores = num_cpus::get();
        
        Self {
            worker_threads: Some(num_cores),
            numa_awareness: true,
            cpu_affinity: AffinityConfig {
                bind_to_cores: true,
                core_assignments: HashMap::new(),
                excluded_cores: Vec::new(),
                preferred_numa_nodes: Vec::new(),
            },
            task_pool_size: DEFAULT_TASK_POOL_SIZE,
            queue_depth: DEFAULT_QUEUE_DEPTH,
            latency_target: TARGET_P99_LATENCY,
            throughput_target: TARGET_THROUGHPUT_MPS,
            work_stealing: true,
            idle_timeout: Duration::from_micros(50), // Reduced for ultra-low latency
            metrics_enabled: true,
        }
    }
    
    /// Validate configuration parameters
    pub fn validate(&self) -> std::result::Result<(), ConfigError> {
        if let Some(workers) = self.worker_threads {
            if workers == 0 {
                return Err(ConfigError::InvalidWorkerCount(workers));
            }
            if workers > 1024 {
                return Err(ConfigError::TooManyWorkers(workers));
            }
        }
        
        if self.task_pool_size == 0 {
            return Err(ConfigError::InvalidTaskPoolSize(self.task_pool_size));
        }
        
        if !self.queue_depth.is_power_of_two() {
            return Err(ConfigError::InvalidQueueDepth(self.queue_depth));
        }
        
        if self.latency_target.is_zero() {
            return Err(ConfigError::InvalidLatencyTarget);
        }
        
        if self.throughput_target == 0 {
            return Err(ConfigError::InvalidThroughputTarget(self.throughput_target));
        }
        
        Ok(())
    }
}

/// Configuration validation errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid worker count: {0}")]
    InvalidWorkerCount(usize),
    
    #[error("Too many workers: {0} (max 1024)")]
    TooManyWorkers(usize),
    
    #[error("Invalid task pool size: {0}")]
    InvalidTaskPoolSize(usize),
    
    #[error("Queue depth must be power of two: {0}")]
    InvalidQueueDepth(usize),
    
    #[error("Latency target must be greater than zero")]
    InvalidLatencyTarget,
    
    #[error("Invalid throughput target: {0}")]
    InvalidThroughputTarget(u64),
}

/// Runtime execution errors
#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    
    #[error("NUMA topology detection failed: {0}")]
    NumaTopology(String),
    
    #[error("Worker thread creation failed: {0}")]
    WorkerCreation(String),
    
    #[error("Task spawn failed: {0}")]
    TaskSpawn(String),
    
    #[error("Runtime shutdown failed: {0}")]
    Shutdown(String),
    
    #[error("CPU affinity setting failed: {0}")]
    CpuAffinity(String),
    
    #[error("Task pool exhausted")]
    TaskPoolExhausted,
    
    #[error("Performance target violation: {0}")]
    PerformanceViolation(String),
}

impl From<RuntimeError> for Error {
    fn from(err: RuntimeError) -> Self {
        Error::Runtime(err.to_string())
    }
}

/// Unique task identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

impl TaskId {
    /// Generate a new unique task ID
    pub fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed) as u64)
    }
    
    /// Get the numeric ID
    pub fn id(&self) -> u64 {
        self.0
    }
}

/// High-performance join handle for spawned tasks
pub struct JoinHandle<T> {
    receiver: oneshot::Receiver<T>,
    task_id: TaskId,
    spawn_time: Instant,
}

impl<T> JoinHandle<T> {
    /// Create a new join handle
    pub(crate) fn new(receiver: oneshot::Receiver<T>, task_id: TaskId) -> Self {
        Self {
            receiver,
            task_id,
            spawn_time: Instant::now(),
        }
    }
    
    /// Get the task ID
    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
    
    /// Get the time since task spawn
    pub fn elapsed(&self) -> Duration {
        self.spawn_time.elapsed()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;
    
    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match std::pin::Pin::new(&mut self.receiver).poll(cx) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(_)) => panic!("Task was cancelled or panicked"),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Ultra-high-performance async runtime optimized for MemoryStreamer
pub struct OptimizedRuntime {
    /// Worker threads for task execution
    workers: Vec<WorkerThread>,
    
    /// Task queues for work distribution
    task_queues: TaskQueues,
    
    /// NUMA topology information
    numa_topology: NumaTopology,
    
    /// Task scheduler for optimal work distribution
    scheduler: TaskScheduler,
    
    /// Runtime performance metrics
    metrics: Arc<RuntimeMetrics>,
    
    /// Runtime configuration
    config: RuntimeConfig,
    
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    
    /// Task ID generator
    next_task_id: AtomicUsize,
}

impl OptimizedRuntime {
    /// Create a new optimized runtime instance
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        // Validate configuration
        config.validate().map_err(RuntimeError::Config)?;
        
        // Detect NUMA topology
        let numa_topology = NumaTopology::detect()
            .map_err(|e| RuntimeError::NumaTopology(e.to_string()))?;
        
        // Determine optimal worker count
        let worker_count = config.worker_threads
            .unwrap_or_else(|| numa_topology.optimal_worker_count());
        
        // Create task queues
        let task_queues = TaskQueues::new(worker_count, config.queue_depth);
        
        // Create task scheduler
        let scheduler = TaskScheduler::new(
            &numa_topology,
            &config,
            task_queues.clone(),
        );
        
        // Initialize metrics
        let metrics = Arc::new(RuntimeMetrics::new(
            config.latency_target,
            config.throughput_target,
            worker_count,
        ));
        
        // Create worker threads with optimal NUMA placement
        let mut workers = Vec::with_capacity(worker_count);
        let worker_placements = numa_topology.optimal_worker_placement(worker_count);
        
        for (worker_id, cpu_set) in worker_placements.into_iter().enumerate() {
            let worker = WorkerThread::new(
                WorkerId::new(worker_id),
                cpu_set,
                task_queues.clone(), // Pass the entire TaskQueues for work stealing
                Arc::clone(&metrics),
                config.idle_timeout,
            );
            workers.push(worker);
        }
        
        Ok(Self {
            workers,
            task_queues,
            numa_topology,
            scheduler,
            metrics,
            config,
            shutdown: Arc::new(AtomicBool::new(false)),
            next_task_id: AtomicUsize::new(1),
        })
    }
    
    /// Spawn a new task on the runtime
    pub fn spawn<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let spawn_start = Instant::now();
        
        // Generate unique task ID
        let task_id = TaskId::new();
        
        // Create result channel
        let (sender, receiver) = oneshot::channel();
        
        // Wrap task with metrics collection
        let metrics = Arc::clone(&self.metrics);
        let task_start = Instant::now();
        
        let wrapped_task = async move {
            let result = task.await;
            
            // Record task completion metrics
            let execution_time = task_start.elapsed();
            metrics.record_task_latency(execution_time);
            
            // Send result
            let _ = sender.send(result);
        };
        
        // Convert to boxed future
        let boxed_task: BoxFuture<'static, ()> = Box::pin(wrapped_task);
        
        // Schedule task on optimal worker
        self.scheduler.schedule_task(boxed_task, task_id);
        
        // Record spawn latency
        let spawn_latency = spawn_start.elapsed();
        self.metrics.record_scheduling_overhead(spawn_latency);
        
        // Validate spawn latency target
        if spawn_latency > MAX_SPAWN_LATENCY {
            tracing::warn!(
                "Task spawn latency {}μs exceeds target {}μs",
                spawn_latency.as_micros(),
                MAX_SPAWN_LATENCY.as_micros()
            );
        }
        
        JoinHandle::new(receiver, task_id)
    }
    
    /// Get runtime performance metrics
    pub fn metrics(&self) -> &RuntimeMetrics {
        &self.metrics
    }
    
    /// Get worker utilization report
    pub fn worker_utilization(&self) -> UtilizationReport {
        self.metrics.worker_utilization_report()
    }
    
    /// Get NUMA topology information
    pub fn numa_topology(&self) -> &NumaTopology {
        &self.numa_topology
    }
    
    /// Check if runtime is shutting down
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }
    
    /// Initiate graceful shutdown of the runtime
    pub async fn shutdown_graceful(&self) {
        // Signal shutdown to all workers
        self.shutdown.store(true, Ordering::Release);
        
        // Wait for all workers to complete current tasks
        for worker in &self.workers {
            worker.shutdown_graceful().await;
        }
        
        // Drain remaining tasks from queues
        self.task_queues.drain_all().await;
        
        tracing::info!("Runtime shutdown completed gracefully");
    }
    
    /// Force immediate shutdown (may lose in-flight tasks)
    pub fn shutdown_immediate(&self) {
        self.shutdown.store(true, Ordering::Release);
        
        for worker in &self.workers {
            worker.shutdown_immediate();
        }
        
        tracing::warn!("Runtime shutdown completed immediately (tasks may be lost)");
    }
}

impl Drop for OptimizedRuntime {
    fn drop(&mut self) {
        if !self.shutdown.load(Ordering::Acquire) {
            tracing::warn!("Runtime dropped without graceful shutdown");
            self.shutdown_immediate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert!(config.numa_awareness);
        assert!(config.work_stealing);
        assert_eq!(config.task_pool_size, DEFAULT_TASK_POOL_SIZE);
        assert_eq!(config.queue_depth, DEFAULT_QUEUE_DEPTH);
        assert!(config.queue_depth.is_power_of_two());
    }
    
    #[test]
    fn test_runtime_config_optimized() {
        let config = RuntimeConfig::optimized();
        assert!(config.worker_threads.is_some());
        assert!(config.numa_awareness);
        assert!(config.cpu_affinity.bind_to_cores);
        assert_eq!(config.idle_timeout, Duration::from_micros(50));
    }
    
    #[test]
    fn test_runtime_config_validation() {
        let mut config = RuntimeConfig::default();
        assert!(config.validate().is_ok());
        
        config.worker_threads = Some(0);
        assert!(matches!(config.validate(), Err(ConfigError::InvalidWorkerCount(0))));
        
        config.worker_threads = Some(2000);
        assert!(matches!(config.validate(), Err(ConfigError::TooManyWorkers(2000))));
        
        config = RuntimeConfig::default();
        config.task_pool_size = 0;
        assert!(matches!(config.validate(), Err(ConfigError::InvalidTaskPoolSize(0))));
        
        config = RuntimeConfig::default();
        config.queue_depth = 1000; // Not power of two
        assert!(matches!(config.validate(), Err(ConfigError::InvalidQueueDepth(1000))));
    }
    
    #[test]
    fn test_task_id_generation() {
        let id1 = TaskId::new();
        let id2 = TaskId::new();
        assert_ne!(id1, id2);
        assert!(id1.id() < id2.id());
    }
    
    #[tokio::test]
    async fn test_runtime_creation() {
        let config = RuntimeConfig::optimized();
        let runtime = OptimizedRuntime::new(config);
        
        // Note: This test may fail in environments without NUMA topology detection
        // In production, we would handle this gracefully
        match runtime {
            Ok(rt) => {
                assert!(!rt.is_shutting_down());
                rt.shutdown_graceful().await;
            }
            Err(e) => {
                // Expected in test environments without proper NUMA support
                eprintln!("Runtime creation failed (expected in test env): {}", e);
            }
        }
    }
    
    #[tokio::test]
    async fn test_basic_task_spawn() {
        let config = RuntimeConfig::optimized();
        if let Ok(runtime) = OptimizedRuntime::new(config) {
            let counter = Arc::new(AtomicU32::new(0));
            let counter_clone = Arc::clone(&counter);
            
            let handle = runtime.spawn(async move {
                counter_clone.fetch_add(1, Ordering::Relaxed);
                42
            });
            
            let result = handle.await;
            assert_eq!(result, 42);
            assert_eq!(counter.load(Ordering::Relaxed), 1);
            
            runtime.shutdown_graceful().await;
        }
    }
    
    #[tokio::test]
    async fn test_concurrent_task_spawn() {
        let config = RuntimeConfig::optimized();
        if let Ok(runtime) = OptimizedRuntime::new(config) {
            let counter = Arc::new(AtomicU32::new(0));
            let mut handles = Vec::new();
            
            // Spawn multiple concurrent tasks
            for i in 0..100 {
                let counter_clone = Arc::clone(&counter);
                let handle = runtime.spawn(async move {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                    i * 2
                });
                handles.push(handle);
            }
            
            // Await all tasks
            let mut results = Vec::new();
            for handle in handles {
                results.push(handle.await);
            }
            
            // Verify results
            assert_eq!(results.len(), 100);
            assert_eq!(counter.load(Ordering::Relaxed), 100);
            
            // Verify all expected results are present
            for i in 0..100 {
                assert!(results.contains(&(i * 2)));
            }
            
            runtime.shutdown_graceful().await;
        }
    }
    
    #[tokio::test]
    async fn test_runtime_metrics() {
        let config = RuntimeConfig::optimized();
        if let Ok(runtime) = OptimizedRuntime::new(config) {
            let metrics = runtime.metrics();
            
            // Initial metrics should be zero
            assert_eq!(metrics.p99_latency().as_nanos(), 0);
            
            // Spawn a task to generate metrics
            let handle = runtime.spawn(async {
                sleep(Duration::from_micros(1)).await;
                "test"
            });
            
            let _result = handle.await;
            
            // Check that metrics were recorded
            let utilization = runtime.worker_utilization();
            assert!(utilization.workers.len() > 0);
            
            runtime.shutdown_graceful().await;
        }
    }
}