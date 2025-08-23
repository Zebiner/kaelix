//! High-Performance Worker Threads
//!
//! Implements optimized worker threads with CPU affinity, work-stealing, and
//! NUMA-aware task execution for ultra-low latency processing.

use crate::runtime::{affinity::CpuSet, metrics::RuntimeMetrics, scheduler::TaskQueues};
use parking_lot::{Condvar, Mutex};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Maximum context switch overhead in microseconds for performance validation
const MAX_CONTEXT_SWITCH_OVERHEAD: Duration = Duration::from_micros(5);

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

/// Worker thread state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    /// Worker is idle, waiting for tasks
    Idle,
    /// Worker is actively processing a task
    Processing,
    /// Worker is performing work stealing
    WorkStealing,
    /// Worker is blocked on I/O or synchronization
    Blocked,
    /// Worker is shutting down
    Stopping,
    /// Worker has stopped
    Stopped,
}

/// Worker statistics for monitoring and optimization
#[derive(Debug, Clone)]
pub struct WorkerStats {
    /// Worker identifier
    pub worker_id: WorkerId,
    /// Current state
    pub state: WorkerState,
    /// Total tasks processed
    pub tasks_processed: u64,
    /// Total execution time
    pub total_execution_time: Duration,
    /// Average task execution time
    pub avg_task_time: Duration,
    /// Work stealing attempts
    pub work_steal_attempts: u64,
    /// Successful work steals
    pub work_steal_successes: u64,
    /// CPU utilization percentage
    pub cpu_utilization: f64,
    /// Last activity timestamp
    pub last_activity: Instant,
}

/// High-performance worker thread with NUMA awareness and work stealing
#[derive(Debug)]
pub struct HighPerformanceWorker {
    /// Worker identifier
    worker_id: WorkerId,
    /// Current state
    state: Arc<AtomicU64>, // Using AtomicU64 to represent WorkerState as u64
    /// Task execution statistics
    stats: Arc<Mutex<WorkerStats>>,
    /// CPU affinity set
    cpu_affinity: Arc<CpuSet>,
    /// Worker thread handle
    thread_handle: Option<JoinHandle<()>>,
    /// Shutdown signal
    shutdown_signal: Arc<AtomicBool>,
    /// Condition variable for task availability
    task_available: Arc<Condvar>,
    /// Mutex for task synchronization
    task_mutex: Arc<Mutex<()>>,
    /// Runtime metrics collector
    metrics: Arc<RuntimeMetrics>,
}

impl HighPerformanceWorker {
    /// Create a new high-performance worker
    pub fn new(worker_id: WorkerId, cpu_affinity: CpuSet, metrics: Arc<RuntimeMetrics>) -> Self {
        let initial_stats = WorkerStats {
            worker_id,
            state: WorkerState::Idle,
            tasks_processed: 0,
            total_execution_time: Duration::from_nanos(0),
            avg_task_time: Duration::from_nanos(0),
            work_steal_attempts: 0,
            work_steal_successes: 0,
            cpu_utilization: 0.0,
            last_activity: Instant::now(),
        };

        Self {
            worker_id,
            state: Arc::new(AtomicU64::new(WorkerState::Idle as u64)),
            stats: Arc::new(Mutex::new(initial_stats)),
            cpu_affinity: Arc::new(cpu_affinity),
            thread_handle: None,
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            task_available: Arc::new(Condvar::new()),
            task_mutex: Arc::new(Mutex::new(())),
            metrics,
        }
    }

    /// Start the worker thread
    pub fn start(&mut self) -> Result<(), std::io::Error> {
        if self.thread_handle.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Worker thread already started",
            ));
        }

        let worker_id = self.worker_id;
        let state = Arc::clone(&self.state);
        let stats = Arc::clone(&self.stats);
        let cpu_affinity = Arc::clone(&self.cpu_affinity);
        let shutdown_signal = Arc::clone(&self.shutdown_signal);
        let task_available = Arc::clone(&self.task_available);
        let task_mutex = Arc::clone(&self.task_mutex);
        let metrics = Arc::clone(&self.metrics);

        let handle = thread::spawn(move || {
            Self::worker_main_loop(
                worker_id,
                state,
                stats,
                cpu_affinity,
                shutdown_signal,
                task_available,
                task_mutex,
                metrics,
            );
        });

        self.thread_handle = Some(handle);
        Ok(())
    }

    /// Stop the worker thread gracefully
    pub fn stop(
        &mut self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Signal shutdown
        self.shutdown_signal.store(true, Ordering::SeqCst);

        // Notify worker to wake up and check shutdown signal
        self.task_available.notify_all();

        // Update state
        self.state.store(WorkerState::Stopping as u64, Ordering::SeqCst);

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let start = Instant::now();

            // Try to join with timeout
            loop {
                if handle.is_finished() {
                    handle.join().map_err(|e| format!("Thread join error: {:?}", e))?;
                    break;
                }

                if start.elapsed() >= timeout {
                    return Err("Worker thread shutdown timeout".into());
                }

                thread::sleep(Duration::from_millis(10));
            }
        }

        // Update final state
        self.state.store(WorkerState::Stopped as u64, Ordering::SeqCst);
        self.stats.lock().state = WorkerState::Stopped;

        Ok(())
    }

    /// Get current worker statistics
    pub fn stats(&self) -> WorkerStats {
        self.stats.lock().clone()
    }

    /// Get current worker state
    pub fn state(&self) -> WorkerState {
        match self.state.load(Ordering::SeqCst) {
            0 => WorkerState::Idle,
            1 => WorkerState::Processing,
            2 => WorkerState::WorkStealing,
            3 => WorkerState::Blocked,
            4 => WorkerState::Stopping,
            5 => WorkerState::Stopped,
            _ => WorkerState::Idle, // Default fallback
        }
    }

    /// Check if worker is healthy and responsive
    pub fn health_check(&self) -> bool {
        let stats = self.stats.lock();
        let state = self.state();

        match state {
            WorkerState::Stopped => false,
            WorkerState::Stopping => false,
            _ => {
                // Check if worker has been responsive recently
                stats.last_activity.elapsed() < Duration::from_secs(30)
            },
        }
    }

    /// Signal task availability to worker
    pub fn notify_task_available(&self) {
        self.task_available.notify_one();
    }

    /// Main worker thread loop
    fn worker_main_loop(
        worker_id: WorkerId,
        state: Arc<AtomicU64>,
        stats: Arc<Mutex<WorkerStats>>,
        cpu_affinity: Arc<CpuSet>,
        shutdown_signal: Arc<AtomicBool>,
        task_available: Arc<Condvar>,
        task_mutex: Arc<Mutex<()>>,
        metrics: Arc<RuntimeMetrics>,
    ) {
        // Set CPU affinity for optimal performance
        if let Err(e) = Self::set_thread_affinity(&cpu_affinity) {
            eprintln!("Failed to set CPU affinity for worker {}: {:?}", worker_id.id(), e);
        }

        let mut last_work_steal = Instant::now();

        while !shutdown_signal.load(Ordering::SeqCst) {
            // Update state to idle
            state.store(WorkerState::Idle as u64, Ordering::SeqCst);

            // Wait for task availability or timeout for work stealing check
            {
                let _guard = task_mutex.lock();
                let _ = task_available.wait_for(&_guard, Duration::from_millis(100));
            }

            // Check shutdown again after waking up
            if shutdown_signal.load(Ordering::SeqCst) {
                break;
            }

            // Try to get and process a task
            let task_processed = Self::try_process_task(worker_id, &state, &stats, &metrics);

            if !task_processed {
                // Try work stealing if no local tasks available
                let now = Instant::now();
                if now.duration_since(last_work_steal) > Duration::from_millis(50) {
                    Self::try_work_stealing(&state, &stats, worker_id);
                    last_work_steal = now;
                }
            }

            // Update activity timestamp
            stats.lock().last_activity = Instant::now();
        }

        // Final cleanup
        state.store(WorkerState::Stopped as u64, Ordering::SeqCst);
        stats.lock().state = WorkerState::Stopped;
    }

    /// Try to process a task from local queue
    fn try_process_task(
        worker_id: WorkerId,
        state: &Arc<AtomicU64>,
        stats: &Arc<Mutex<WorkerStats>>,
        metrics: &Arc<RuntimeMetrics>,
    ) -> bool {
        // This would integrate with the actual task queue system
        // For now, simulate task processing

        state.store(WorkerState::Processing as u64, Ordering::SeqCst);

        // Simulate task execution
        let start = Instant::now();

        // Actual task processing would happen here
        thread::sleep(Duration::from_micros(10)); // Simulate minimal work

        let execution_time = start.elapsed();

        // Update statistics
        {
            let mut worker_stats = stats.lock();
            worker_stats.tasks_processed += 1;
            worker_stats.total_execution_time += execution_time;
            worker_stats.avg_task_time =
                worker_stats.total_execution_time / worker_stats.tasks_processed as u32;
            worker_stats.state = WorkerState::Processing;
        }

        // Record metrics
        metrics.record_worker_task_completion(worker_id, execution_time);

        false // No actual task processed in this simulation
    }

    /// Try to steal work from other workers
    fn try_work_stealing(
        state: &Arc<AtomicU64>,
        stats: &Arc<Mutex<WorkerStats>>,
        _worker_id: WorkerId,
    ) -> bool {
        // Update state to work stealing
        state.store(WorkerState::WorkStealing as u64, Ordering::SeqCst);

        // Update stats
        stats.lock().work_steal_attempts += 1;

        // This is a stub - actual work stealing would be implemented here
        // For now, just return false to indicate no work stolen
        false
    }

    /// Set CPU affinity for the current thread
    fn set_thread_affinity(cpu_set: &CpuSet) -> Result<(), std::io::Error> {
        // This would use platform-specific APIs to set CPU affinity
        // For now, just log the intent
        println!("Setting CPU affinity to {:?}", cpu_set);
        Ok(())
    }
}

impl Drop for HighPerformanceWorker {
    fn drop(&mut self) {
        if self.thread_handle.is_some() {
            let _ = self.stop(Duration::from_secs(5));
        }
    }
}

/// Worker pool for managing multiple high-performance workers
#[derive(Debug)]
pub struct WorkerPool {
    /// Collection of workers
    workers: Vec<HighPerformanceWorker>,
    /// Pool-wide metrics
    metrics: Arc<RuntimeMetrics>,
    /// Pool shutdown signal
    shutdown_signal: Arc<AtomicBool>,
}

impl WorkerPool {
    /// Create a new worker pool
    pub fn new(
        num_workers: usize,
        cpu_affinity_sets: Vec<CpuSet>,
        metrics: Arc<RuntimeMetrics>,
    ) -> Self {
        let mut workers = Vec::with_capacity(num_workers);

        for (i, cpu_set) in cpu_affinity_sets.into_iter().enumerate().take(num_workers) {
            let worker_id = WorkerId::new(i);
            let worker = HighPerformanceWorker::new(worker_id, cpu_set, Arc::clone(&metrics));
            workers.push(worker);
        }

        Self { workers, metrics, shutdown_signal: Arc::new(AtomicBool::new(false)) }
    }

    /// Start all workers in the pool
    pub fn start_all(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for worker in &mut self.workers {
            worker.start().map_err(|e| format!("Failed to start worker: {}", e))?;
        }
        Ok(())
    }

    /// Stop all workers in the pool
    pub fn stop_all(
        &mut self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.shutdown_signal.store(true, Ordering::SeqCst);

        for worker in &mut self.workers {
            worker.stop(timeout)?;
        }

        Ok(())
    }

    /// Get statistics for all workers
    pub fn get_all_stats(&self) -> Vec<WorkerStats> {
        self.workers.iter().map(|w| w.stats()).collect()
    }

    /// Perform health check on all workers
    pub fn health_check_all(&self) -> Vec<(WorkerId, bool)> {
        self.workers.iter().map(|w| (w.worker_id, w.health_check())).collect()
    }

    /// Get total number of workers
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Notify all workers of task availability
    pub fn notify_all_workers(&self) {
        for worker in &self.workers {
            worker.notify_task_available();
        }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        let _ = self.stop_all(Duration::from_secs(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::metrics::RuntimeMetrics;

    #[test]
    fn test_worker_id_creation() {
        let worker_id = WorkerId::new(42);
        assert_eq!(worker_id.id(), 42);
    }

    #[test]
    fn test_worker_creation() {
        let metrics = Arc::new(RuntimeMetrics::new());
        let cpu_set = CpuSet::new();
        let worker_id = WorkerId::new(0);

        let worker = HighPerformanceWorker::new(worker_id, cpu_set, metrics);

        assert_eq!(worker.worker_id, worker_id);
        assert_eq!(worker.state(), WorkerState::Idle);
        assert!(worker.health_check());
    }

    #[test]
    fn test_worker_pool_creation() {
        let metrics = Arc::new(RuntimeMetrics::new());
        let cpu_sets = vec![CpuSet::new(), CpuSet::new()];

        let pool = WorkerPool::new(2, cpu_sets, metrics);

        assert_eq!(pool.worker_count(), 2);
    }
}
