//! High-Performance Worker Threads
//!
//! Implements optimized worker threads with CPU affinity, work-stealing, and
//! NUMA-aware task execution for ultra-low latency processing.

use crate::runtime::{
    affinity::CpuSet,
    metrics::RuntimeMetrics,
    scheduler::{Task, TaskQueues},
    performance::*,
};
use crossbeam::queue::SegQueue;
use futures::{
    task::{Context, Poll, Waker},
    Future,
};
use parking_lot::{Condvar, Mutex};
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    task::Wake,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Worker thread identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkerId {
    id: usize,
}

impl WorkerId {
    /// Create a new worker ID
    pub fn new(id: usize) -> Self {
        Self { id }
    }
    
    /// Get the numeric ID
    pub fn id(&self) -> usize {
        self.id
    }
}

impl std::fmt::Display for WorkerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "worker{}", self.id)
    }
}

/// Worker thread statistics
#[derive(Debug)]
pub struct WorkerMetrics {
    /// Number of tasks executed
    pub tasks_executed: AtomicU64,
    
    /// Total execution time
    pub total_execution_time: AtomicU64,
    
    /// Number of successful work steals
    pub successful_steals: AtomicU64,
    
    /// Number of failed work steal attempts
    pub failed_steals: AtomicU64,
    
    /// Time spent idle (parked)
    pub idle_time: AtomicU64,
    
    /// Time spent actively working
    pub active_time: AtomicU64,
    
    /// Last activity timestamp
    pub last_activity: AtomicU64,
    
    /// Current queue depth
    pub queue_depth: AtomicUsize,
}

impl WorkerMetrics {
    /// Create new worker metrics
    pub fn new() -> Self {
        Self {
            tasks_executed: AtomicU64::new(0),
            total_execution_time: AtomicU64::new(0),
            successful_steals: AtomicU64::new(0),
            failed_steals: AtomicU64::new(0),
            idle_time: AtomicU64::new(0),
            active_time: AtomicU64::new(0),
            last_activity: AtomicU64::new(0),
            queue_depth: AtomicUsize::new(0),
        }
    }
    
    /// Record task execution
    pub fn record_task_execution(&self, duration: Duration) {
        self.tasks_executed.fetch_add(1, Ordering::Relaxed);
        self.total_execution_time.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        self.last_activity.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            Ordering::Relaxed,
        );
    }
    
    /// Record successful work steal
    pub fn record_successful_steal(&self) {
        self.successful_steals.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record failed work steal attempt
    pub fn record_failed_steal(&self) {
        self.failed_steals.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record idle period
    pub fn record_idle_time(&self, duration: Duration) {
        self.idle_time.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
    
    /// Record active period
    pub fn record_active_time(&self, duration: Duration) {
        self.active_time.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }
    
    /// Update queue depth
    pub fn update_queue_depth(&self, depth: usize) {
        self.queue_depth.store(depth, Ordering::Relaxed);
    }
    
    /// Get average task execution time
    pub fn average_execution_time(&self) -> Duration {
        let total_tasks = self.tasks_executed.load(Ordering::Relaxed);
        if total_tasks == 0 {
            Duration::ZERO
        } else {
            let total_time = self.total_execution_time.load(Ordering::Relaxed);
            Duration::from_nanos(total_time / total_tasks)
        }
    }
    
    /// Get work steal success rate
    pub fn steal_success_rate(&self) -> f64 {
        let successful = self.successful_steals.load(Ordering::Relaxed);
        let failed = self.failed_steals.load(Ordering::Relaxed);
        let total = successful + failed;
        
        if total == 0 {
            0.0
        } else {
            successful as f64 / total as f64
        }
    }
    
    /// Get worker utilization (active time / total time)
    pub fn utilization(&self) -> f64 {
        let active = self.active_time.load(Ordering::Relaxed);
        let idle = self.idle_time.load(Ordering::Relaxed);
        let total = active + idle;
        
        if total == 0 {
            0.0
        } else {
            active as f64 / total as f64
        }
    }
}

impl Default for WorkerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Parker for efficiently parking/unparking worker threads
pub struct Parker {
    /// Mutex and condition variable for parking
    inner: Arc<Mutex<bool>>,
    condvar: Arc<Condvar>,
    
    /// Unpark handle for other threads to wake this worker
    unparker: Unparker,
}

impl Parker {
    /// Create a new parker
    pub fn new() -> Self {
        let inner = Arc::new(Mutex::new(false));
        let condvar = Arc::new(Condvar::new());
        
        let unparker = Unparker {
            inner: inner.clone(),
            condvar: condvar.clone(),
        };
        
        Self {
            inner,
            condvar,
            unparker,
        }
    }
    
    /// Park the current thread with optional timeout
    pub fn park_timeout(&self, timeout: Duration) -> bool {
        let mut guard = self.inner.lock();
        
        if *guard {
            *guard = false;
            true // Was already unparked
        } else {
            let result = self.condvar.wait_for(&mut guard, timeout);
            
            if *guard {
                *guard = false;
                true // Unparked by another thread
            } else {
                !result.timed_out() // Timeout occurred
            }
        }
    }
    
    /// Park the current thread indefinitely
    pub fn park(&self) {
        let mut guard = self.inner.lock();
        
        while !*guard {
            self.condvar.wait(&mut guard);
        }
        
        *guard = false;
    }
    
    /// Get the unparker handle
    pub fn unparker(&self) -> Unparker {
        self.unparker.clone()
    }
}

impl Default for Parker {
    fn default() -> Self {
        Self::new()
    }
}

/// Unparker handle for waking up a parked worker
#[derive(Clone)]
pub struct Unparker {
    inner: Arc<Mutex<bool>>,
    condvar: Arc<Condvar>,
}

impl Unparker {
    /// Unpark the associated worker thread
    pub fn unpark(&self) {
        let mut guard = self.inner.lock();
        *guard = true;
        self.condvar.notify_one();
    }
}

/// Handle for stealing work from other workers
pub struct StealHandle {
    /// Other workers' local queues for stealing
    steal_queues: Vec<Arc<SegQueue<Task>>>,
    
    /// Current steal index (round-robin)
    current_index: AtomicUsize,
    
    /// Maximum steal attempts per operation
    max_attempts: usize,
}

impl StealHandle {
    /// Create a new steal handle
    pub fn new(steal_queues: Vec<Arc<SegQueue<Task>>>, max_attempts: usize) -> Self {
        Self {
            steal_queues,
            current_index: AtomicUsize::new(0),
            max_attempts,
        }
    }
    
    /// Attempt to steal work from other workers
    pub fn steal_work(&self) -> Option<Task> {
        if self.steal_queues.is_empty() {
            return None;
        }
        
        let start_index = self.current_index.load(Ordering::Relaxed);
        let mut attempts = 0;
        
        while attempts < self.max_attempts && attempts < self.steal_queues.len() {
            let index = (start_index + attempts) % self.steal_queues.len();
            
            if let Some(task) = self.steal_queues[index].pop() {
                // Update the starting index for next time
                self.current_index.store((index + 1) % self.steal_queues.len(), Ordering::Relaxed);
                return Some(task);
            }
            
            attempts += 1;
        }
        
        None
    }
}

/// High-performance worker thread
pub struct WorkerThread {
    /// Worker identifier
    id: WorkerId,
    
    /// CPU affinity assignment
    cpu_affinity: CpuSet,
    
    /// Local task queue
    local_queue: Arc<SegQueue<Task>>,
    
    /// Work stealing handle
    steal_handle: StealHandle,
    
    /// Thread parker for efficient waiting
    parker: Parker,
    
    /// Worker-specific metrics
    metrics: WorkerMetrics,
    
    /// Runtime metrics reference
    runtime_metrics: Arc<RuntimeMetrics>,
    
    /// Idle timeout before parking
    idle_timeout: Duration,
    
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    
    /// Thread handle
    thread_handle: Option<JoinHandle<()>>,
    
    /// Task execution context
    context: TaskContext,
}

/// Task execution context for a worker
struct TaskContext {
    /// Waker for task notifications
    waker: Waker,
    
    /// Ready queue for tasks that become ready during execution
    ready_queue: VecDeque<Task>,
}

impl TaskContext {
    fn new(worker_id: WorkerId) -> Self {
        let waker = Arc::new(TaskWaker {
            worker_id,
            // TODO: Add channel to notify worker of ready tasks
        }).into();
        
        Self {
            waker,
            ready_queue: VecDeque::new(),
        }
    }
}

/// Custom waker implementation for worker tasks
struct TaskWaker {
    worker_id: WorkerId,
    // TODO: Add notification mechanism
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        // TODO: Implement wake notification to worker
        // For now, this is a placeholder
    }
    
    fn wake_by_ref(self: &Arc<Self>) {
        // TODO: Implement wake notification to worker
        // For now, this is a placeholder
    }
}

impl WorkerThread {
    /// Create a new worker thread
    pub fn new(
        id: WorkerId,
        cpu_affinity: CpuSet,
        task_queues: TaskQueues,
        runtime_metrics: Arc<RuntimeMetrics>,
        idle_timeout: Duration,
    ) -> Self {
        let local_queue = task_queues.local_queue(id.id());
        
        // Get all other local queues for work stealing (excluding our own)
        let mut steal_queues = Vec::new();
        for worker_id in 0..task_queues.all_local_queues().len() {
            if worker_id != id.id() {
                steal_queues.push(task_queues.local_queue(worker_id));
            }
        }
        
        let steal_handle = StealHandle::new(steal_queues, 3);
        let parker = Parker::new();
        let context = TaskContext::new(id);
        
        Self {
            id,
            cpu_affinity,
            local_queue,
            steal_handle,
            parker,
            metrics: WorkerMetrics::new(),
            runtime_metrics,
            idle_timeout,
            shutdown: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            context,
        }
    }
    
    /// Start the worker thread
    pub fn start(&mut self) -> Result<(), std::io::Error> {
        let id = self.id;
        let cpu_affinity = self.cpu_affinity.clone();
        let local_queue = self.local_queue.clone();
        let steal_handle = StealHandle::new(
            Vec::new(), // Will be populated with actual steal queues
            3,
        );
        let parker = Parker::new();
        let metrics = WorkerMetrics::new();
        let runtime_metrics = self.runtime_metrics.clone();
        let idle_timeout = self.idle_timeout;
        let shutdown = self.shutdown.clone();
        
        let handle = thread::Builder::new()
            .name(format!("kaelix-worker-{}", id.id()))
            .spawn(move || {
                // Set CPU affinity
                if let Err(e) = cpu_affinity.apply_to_current_thread() {
                    tracing::warn!("Failed to set CPU affinity for {}: {}", id, e);
                }
                
                tracing::info!("Worker {} started with CPU affinity: {}", id, cpu_affinity);
                
                let mut context = TaskContext::new(id);
                Self::run_worker_loop(
                    id,
                    local_queue,
                    steal_handle,
                    parker,
                    metrics,
                    runtime_metrics,
                    idle_timeout,
                    shutdown,
                    &mut context,
                );
                
                tracing::info!("Worker {} shutting down", id);
            })?;
        
        self.thread_handle = Some(handle);
        Ok(())
    }
    
    /// Main worker loop
    fn run_worker_loop(
        _id: WorkerId,
        local_queue: Arc<SegQueue<Task>>,
        steal_handle: StealHandle,
        parker: Parker,
        metrics: WorkerMetrics,
        runtime_metrics: Arc<RuntimeMetrics>,
        idle_timeout: Duration,
        shutdown: Arc<AtomicBool>,
        context: &mut TaskContext,
    ) {
        let mut last_activity = Instant::now();
        let mut consecutive_idle_cycles = 0;
        
        while !shutdown.load(Ordering::Acquire) {
            let cycle_start = Instant::now();
            let mut found_work = false;
            
            // Process tasks from ready queue first
            while let Some(mut task) = context.ready_queue.pop_front() {
                Self::execute_task(&mut task, context, &metrics, &runtime_metrics);
                found_work = true;
                
                // Check for shutdown between tasks
                if shutdown.load(Ordering::Acquire) {
                    return;
                }
            }
            
            // Try to get task from local queue
            if let Some(mut task) = local_queue.pop() {
                Self::execute_task(&mut task, context, &metrics, &runtime_metrics);
                found_work = true;
            }
            
            // Try to steal work from other workers
            if !found_work {
                if let Some(mut task) = steal_handle.steal_work() {
                    metrics.record_successful_steal();
                    Self::execute_task(&mut task, context, &metrics, &runtime_metrics);
                    found_work = true;
                } else {
                    metrics.record_failed_steal();
                }
            }
            
            let cycle_time = cycle_start.elapsed();
            
            if found_work {
                consecutive_idle_cycles = 0;
                last_activity = Instant::now();
                metrics.record_active_time(cycle_time);
            } else {
                consecutive_idle_cycles += 1;
                metrics.record_idle_time(cycle_time);
                
                // Park the thread if idle for too long
                if consecutive_idle_cycles > 10 && last_activity.elapsed() > idle_timeout {
                    let park_start = Instant::now();
                    
                    // Park with timeout to avoid missing work
                    let _unparked = parker.park_timeout(idle_timeout);
                    
                    let park_time = park_start.elapsed();
                    metrics.record_idle_time(park_time);
                    
                    // Reset idle counter after parking
                    consecutive_idle_cycles = 0;
                }
            }
            
            // Yield to avoid spinning too aggressively
            if !found_work {
                thread::yield_now();
            }
        }
    }
    
    /// Execute a single task
    fn execute_task(
        task: &mut Task,
        context: &mut TaskContext,
        metrics: &WorkerMetrics,
        runtime_metrics: &Arc<RuntimeMetrics>,
    ) {
        let start_time = Instant::now();
        
        // Take the future from the task for execution
        if let Some(mut future) = task.take_future() {
            // Create polling context
            let mut cx = Context::from_waker(&context.waker);
            
            // Poll the task future
            match Pin::new(&mut future).poll(&mut cx) {
                Poll::Ready(()) => {
                    // Task completed
                    let execution_time = start_time.elapsed();
                    metrics.record_task_execution(execution_time);
                    runtime_metrics.record_task_latency(execution_time);
                    
                    // Record task completion in runtime metrics
                    runtime_metrics.record_task_completion(task.age());
                    
                    // Validate performance targets
                    if execution_time > MAX_CONTEXT_SWITCH_OVERHEAD * 10 {
                        tracing::warn!(
                            "Task {} execution time {}μs exceeds performance target",
                            task.id.id(),
                            execution_time.as_micros()
                        );
                    }
                }
                Poll::Pending => {
                    // Task is not ready, put it back for later
                    // In a real implementation, we would need a way to track pending tasks
                    // and wake them when they become ready
                    tracing::trace!("Task {} is pending, implementing proper async handling needed", task.id.id());
                    
                    // Put the future back for future execution
                    *task.future.lock() = Some(future);
                }
            }
        }
    }
    
    /// Get worker metrics
    pub fn metrics(&self) -> &WorkerMetrics {
        &self.metrics
    }
    
    /// Get worker ID
    pub fn id(&self) -> WorkerId {
        self.id
    }
    
    /// Get CPU affinity
    pub fn cpu_affinity(&self) -> &CpuSet {
        &self.cpu_affinity
    }
    
    /// Check if worker is running
    pub fn is_running(&self) -> bool {
        !self.shutdown.load(Ordering::Acquire)
    }
    
    /// Initiate graceful shutdown
    pub async fn shutdown_graceful(&self) {
        self.shutdown.store(true, Ordering::Release);
        
        // Unpark the worker if it's sleeping
        self.parker.unparker().unpark();
        
        // Wait for the thread to finish
        if let Some(handle) = &self.thread_handle {
            // Note: This is a simplified approach
            // In practice, we'd need to handle this more carefully
            while !handle.is_finished() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    }
    
    /// Force immediate shutdown
    pub fn shutdown_immediate(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.parker.unparker().unpark();
    }
    
    /// Steal tasks from this worker (called by other workers)
    pub fn steal_tasks(&self) -> Vec<Task> {
        // This would be implemented using the stealer from local_queue
        // For now, return empty vec as placeholder
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::scheduler::TaskQueues;
    use futures::future::ready;

    #[test]
    fn test_worker_id() {
        let id = WorkerId::new(42);
        assert_eq!(id.id(), 42);
        assert_eq!(format!("{}", id), "worker42");
    }
    
    #[test]
    fn test_worker_metrics() {
        let metrics = WorkerMetrics::new();
        
        // Test initial state
        assert_eq!(metrics.tasks_executed.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.average_execution_time(), Duration::ZERO);
        assert_eq!(metrics.steal_success_rate(), 0.0);
        assert_eq!(metrics.utilization(), 0.0);
        
        // Test recording task execution
        metrics.record_task_execution(Duration::from_micros(100));
        assert_eq!(metrics.tasks_executed.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.average_execution_time(), Duration::from_micros(100));
        
        // Test recording steals
        metrics.record_successful_steal();
        metrics.record_failed_steal();
        assert_eq!(metrics.steal_success_rate(), 0.5);
        
        // Test utilization calculation
        metrics.record_active_time(Duration::from_millis(60));
        metrics.record_idle_time(Duration::from_millis(40));
        assert_eq!(metrics.utilization(), 0.6);
    }
    
    #[test]
    fn test_parker() {
        let parker = Parker::new();
        
        // Test immediate unpark
        parker.unparker().unpark();
        assert!(parker.park_timeout(Duration::from_millis(1)));
        
        // Test timeout
        let start = Instant::now();
        assert!(!parker.park_timeout(Duration::from_millis(10)));
        assert!(start.elapsed() >= Duration::from_millis(10));
    }
    
    #[test]
    fn test_steal_handle() {
        let steal_queues = Vec::new(); // Empty for test
        let handle = StealHandle::new(steal_queues, 3);
        
        // Should return None with no steal queues
        assert!(handle.steal_work().is_none());
    }
    
    #[tokio::test]
    async fn test_worker_thread_creation() {
        let worker_id = WorkerId::new(0);
        let cpu_set = CpuSet::from_cores(vec![0]);
        let task_queues = TaskQueues::new(2, 1024);
        let runtime_metrics = Arc::new(RuntimeMetrics::new(
            Duration::from_micros(10),
            1000000,
            1,
        ));
        let idle_timeout = Duration::from_micros(100);
        
        let worker = WorkerThread::new(
            worker_id,
            cpu_set,
            task_queues,
            runtime_metrics,
            idle_timeout,
        );
        
        assert_eq!(worker.id(), worker_id);
        assert!(worker.is_running());
        
        // Test graceful shutdown
        worker.shutdown_graceful().await;
        assert!(!worker.is_running());
    }
    
    #[test]
    fn test_task_context() {
        let worker_id = WorkerId::new(0);
        let context = TaskContext::new(worker_id);
        
        assert!(context.ready_queue.is_empty());
        // Waker testing would require more complex setup
    }
}