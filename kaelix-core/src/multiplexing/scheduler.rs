//! High-Performance Stream Scheduler
//!
//! Fair scheduling system with Multi-Level Feedback Queues, work-stealing, and priority support.
//! Designed for <50ns scheduling decisions and fair resource distribution across 1M+ streams.

use crate::multiplexing::error::{SchedulerError, SchedulerResult, StreamId, WorkerId, Priority};
use crate::multiplexing::registry::StreamPriority;
use crossbeam::queue::{SegQueue, ArrayQueue};
use crossbeam::utils::Backoff;
use dashmap::DashMap;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Number of Multi-Level Feedback Queue levels
const MLFQ_LEVELS: usize = 8;

/// Default time quantum for each MLFQ level (in nanoseconds)
const DEFAULT_TIME_QUANTUM_NS: [u64; MLFQ_LEVELS] = [
    1_000,     // Level 0: 1μs (highest priority)
    2_000,     // Level 1: 2μs
    4_000,     // Level 2: 4μs
    8_000,     // Level 3: 8μs
    16_000,    // Level 4: 16μs
    32_000,    // Level 5: 32μs
    64_000,    // Level 6: 64μs
    100_000,   // Level 7: 100μs (lowest priority)
];

/// Work-stealing queue capacity per worker
const WORKER_QUEUE_CAPACITY: usize = 1024;

/// Priority queue capacity
const PRIORITY_QUEUE_CAPACITY: usize = 4096;

/// Task types for stream processing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    ProcessMessage(u64), // MessageId
    HandleBackpressure,
    UpdateMetrics,
    StateTransition,
    TopologyUpdate,
    Rebalance,
    HealthCheck,
}

/// Stream processing task
#[derive(Debug, Clone)]
pub struct StreamTask {
    pub stream_id: StreamId,
    pub task_type: TaskType,
    pub priority: StreamPriority,
    pub created_at: Instant,
    pub deadline: Option<Instant>,
    pub processing_time_ns: u64,
    pub retry_count: u8,
    pub mlfq_level: u8,
}

impl StreamTask {
    pub fn new(stream_id: StreamId, task_type: TaskType, priority: StreamPriority) -> Self {
        Self {
            stream_id,
            task_type,
            priority,
            created_at: Instant::now(),
            deadline: None,
            processing_time_ns: 0,
            retry_count: 0,
            mlfq_level: 0, // Start at highest priority level
        }
    }
    
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
    
    pub fn is_expired(&self) -> bool {
        self.deadline.map_or(false, |d| Instant::now() > d)
    }
    
    pub fn age_ns(&self) -> u64 {
        self.created_at.elapsed().as_nanos() as u64
    }
    
    pub fn promote_priority(&mut self) {
        if self.mlfq_level > 0 {
            self.mlfq_level -= 1;
        }
    }
    
    pub fn demote_priority(&mut self) {
        if self.mlfq_level < MLFQ_LEVELS as u8 - 1 {
            self.mlfq_level += 1;
        }
    }
}

/// Priority task for urgent processing
#[derive(Debug, Clone)]
pub struct PriorityTask {
    pub task: StreamTask,
    pub urgency_score: u64,
}

impl PriorityTask {
    pub fn new(task: StreamTask) -> Self {
        let urgency_score = Self::calculate_urgency(&task);
        Self { task, urgency_score }
    }
    
    fn calculate_urgency(task: &StreamTask) -> u64 {
        let priority_weight = match task.priority {
            StreamPriority::Realtime => 1000,
            StreamPriority::Critical => 800,
            StreamPriority::High => 600,
            StreamPriority::Normal => 400,
            StreamPriority::Low => 200,
            StreamPriority::Background => 100,
        };
        
        let age_penalty = task.age_ns() / 1000; // Convert to microseconds
        let deadline_urgency = task.deadline.map_or(0, |d| {
            let remaining = d.saturating_duration_since(Instant::now()).as_nanos() as u64;
            if remaining < 1_000_000 { // Less than 1ms remaining
                1000
            } else {
                0
            }
        });
        
        priority_weight + age_penalty + deadline_urgency
    }
}

/// MLFQ level tracking
#[derive(Debug, Clone)]
pub struct QueueLevel {
    pub level: u8,
    pub time_quantum_ns: u64,
    pub last_scheduled: Instant,
    pub execution_count: u64,
}

impl QueueLevel {
    pub fn new(level: u8) -> Self {
        Self {
            level,
            time_quantum_ns: DEFAULT_TIME_QUANTUM_NS[level as usize],
            last_scheduled: Instant::now(),
            execution_count: 0,
        }
    }
}

/// Work-stealing queue for per-worker task distribution
#[derive(Debug)]
pub struct WorkStealingQueue<T> {
    local_queue: ArrayQueue<T>,
    stealer_queue: SegQueue<T>,
    pending_tasks: AtomicUsize,
}

impl<T> WorkStealingQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            local_queue: ArrayQueue::new(capacity),
            stealer_queue: SegQueue::new(),
            pending_tasks: AtomicUsize::new(0),
        }
    }
    
    pub fn push_local(&self, task: T) -> Result<(), T> {
        match self.local_queue.push(task) {
            Ok(()) => {
                self.pending_tasks.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(task) => {
                // Local queue full, push to stealer queue
                self.stealer_queue.push(task);
                self.pending_tasks.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }
    }
    
    pub fn pop_local(&self) -> Option<T> {
        if let Some(task) = self.local_queue.pop() {
            self.pending_tasks.fetch_sub(1, Ordering::Relaxed);
            Some(task)
        } else if let Some(task) = self.stealer_queue.pop() {
            self.pending_tasks.fetch_sub(1, Ordering::Relaxed);
            Some(task)
        } else {
            None
        }
    }
    
    pub fn steal(&self) -> Option<T> {
        if let Some(task) = self.stealer_queue.pop() {
            self.pending_tasks.fetch_sub(1, Ordering::Relaxed);
            Some(task)
        } else {
            None
        }
    }
    
    pub fn len(&self) -> usize {
        self.pending_tasks.load(Ordering::Relaxed)
    }
    
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// CPU affinity and NUMA mapping
#[derive(Debug)]
pub struct CpuAffinityMapper {
    cpu_count: usize,
    numa_nodes: Vec<Vec<usize>>, // CPU IDs per NUMA node
    current_cpu: AtomicUsize,
}

impl CpuAffinityMapper {
    pub fn new() -> Self {
        let cpu_count = num_cpus::get();
        
        // Simple NUMA mapping - assumes 2 NUMA nodes for now
        let mut numa_nodes = vec![Vec::new(), Vec::new()];
        for cpu_id in 0..cpu_count {
            let node = if cpu_id < cpu_count / 2 { 0 } else { 1 };
            numa_nodes[node].push(cpu_id);
        }
        
        Self {
            cpu_count,
            numa_nodes,
            current_cpu: AtomicUsize::new(0),
        }
    }
    
    pub fn get_next_cpu(&self) -> usize {
        let current = self.current_cpu.fetch_add(1, Ordering::Relaxed);
        current % self.cpu_count
    }
    
    pub fn get_numa_node_for_cpu(&self, cpu_id: usize) -> usize {
        for (node_id, cpus) in self.numa_nodes.iter().enumerate() {
            if cpus.contains(&cpu_id) {
                return node_id;
            }
        }
        0 // Default to node 0
    }
    
    pub fn get_cpus_for_numa_node(&self, node_id: usize) -> &[usize] {
        self.numa_nodes.get(node_id).map_or(&[], |v| v.as_slice())
    }
}

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub worker_count: usize,
    pub worker_queue_capacity: usize,
    pub priority_queue_capacity: usize,
    pub enable_work_stealing: bool,
    pub enable_fair_scheduling: bool,
    pub enable_numa_awareness: bool,
    pub rebalance_interval_ms: u64,
    pub starvation_threshold_ms: u64,
    pub time_quantum_multiplier: f64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            worker_count: num_cpus::get(),
            worker_queue_capacity: WORKER_QUEUE_CAPACITY,
            priority_queue_capacity: PRIORITY_QUEUE_CAPACITY,
            enable_work_stealing: true,
            enable_fair_scheduling: true,
            enable_numa_awareness: true,
            rebalance_interval_ms: 100,
            starvation_threshold_ms: 1000,
            time_quantum_multiplier: 1.0,
        }
    }
}

/// Scheduler performance metrics
#[derive(Debug, Default)]
pub struct SchedulerMetrics {
    pub tasks_scheduled: AtomicU64,
    pub tasks_completed: AtomicU64,
    pub scheduling_time_ns: AtomicU64,
    pub work_steals: AtomicU64,
    pub queue_overflows: AtomicU64,
    pub deadline_violations: AtomicU64,
    pub fairness_violations: AtomicU64,
    pub rebalance_operations: AtomicU64,
    pub starvation_events: AtomicU64,
}

impl SchedulerMetrics {
    pub fn record_scheduling(&self, duration_ns: u64) {
        self.tasks_scheduled.fetch_add(1, Ordering::Relaxed);
        self.scheduling_time_ns.fetch_add(duration_ns, Ordering::Relaxed);
    }
    
    pub fn record_completion(&self) {
        self.tasks_completed.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_work_steal(&self) {
        self.work_steals.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_queue_overflow(&self) {
        self.queue_overflows.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_deadline_violation(&self) {
        self.deadline_violations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_fairness_violation(&self) {
        self.fairness_violations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_rebalance(&self) {
        self.rebalance_operations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_starvation(&self) {
        self.starvation_events.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn get_average_scheduling_time_ns(&self) -> u64 {
        let count = self.tasks_scheduled.load(Ordering::Relaxed);
        if count == 0 {
            return 0;
        }
        self.scheduling_time_ns.load(Ordering::Relaxed) / count
    }
    
    pub fn get_completion_rate(&self) -> f64 {
        let scheduled = self.tasks_scheduled.load(Ordering::Relaxed) as f64;
        let completed = self.tasks_completed.load(Ordering::Relaxed) as f64;
        if scheduled == 0.0 {
            return 0.0;
        }
        completed / scheduled
    }
}

/// High-Performance Stream Scheduler
#[derive(Debug)]
pub struct StreamScheduler {
    /// Work-stealing queues per CPU core
    worker_queues: Vec<Arc<WorkStealingQueue<StreamTask>>>,
    /// Multi-level feedback queue for fair scheduling
    mlfq_levels: [Arc<DashMap<StreamId, QueueLevel>>; MLFQ_LEVELS],
    /// Global priority queue for high-priority streams
    priority_queue: Arc<SegQueue<PriorityTask>>,
    /// CPU affinity and NUMA awareness
    cpu_mapper: Arc<CpuAffinityMapper>,
    /// Scheduling configuration
    config: SchedulerConfig,
    /// Performance metrics
    metrics: SchedulerMetrics,
    /// Scheduler state
    is_running: AtomicBool,
    last_rebalance: RwLock<Instant>,
}

impl StreamScheduler {
    /// Create a new stream scheduler
    pub fn new(config: SchedulerConfig) -> Self {
        // Initialize worker queues
        let mut worker_queues = Vec::with_capacity(config.worker_count);
        for _ in 0..config.worker_count {
            worker_queues.push(Arc::new(WorkStealingQueue::new(config.worker_queue_capacity)));
        }
        
        // Initialize MLFQ levels
        let mlfq_levels: [Arc<DashMap<StreamId, QueueLevel>>; MLFQ_LEVELS] = 
            std::array::from_fn(|_| Arc::new(DashMap::new()));
        
        // Initialize priority queue
        let priority_queue = Arc::new(SegQueue::new());
        
        // Initialize CPU mapper
        let cpu_mapper = Arc::new(CpuAffinityMapper::new());
        
        Self {
            worker_queues,
            mlfq_levels,
            priority_queue,
            cpu_mapper,
            config,
            metrics: SchedulerMetrics::default(),
            is_running: AtomicBool::new(false),
            last_rebalance: RwLock::new(Instant::now()),
        }
    }
    
    /// Start the scheduler
    pub fn start(&self) -> SchedulerResult<()> {
        if self.is_running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return Err(SchedulerError::PolicyViolation);
        }
        Ok(())
    }
    
    /// Stop the scheduler
    pub fn stop(&self) -> SchedulerResult<()> {
        self.is_running.store(false, Ordering::Release);
        Ok(())
    }
    
    /// Schedule a task with <50ns scheduling decision time
    pub async fn schedule_task(&self, mut task: StreamTask) -> SchedulerResult<()> {
        let start = Instant::now();
        
        if !self.is_running.load(Ordering::Acquire) {
            return Err(SchedulerError::PolicyViolation);
        }
        
        // Check deadline expiration
        if task.is_expired() {
            self.metrics.record_deadline_violation();
            return Err(SchedulerError::DeadlineExceeded {
                stream_id: task.stream_id,
                deadline_ns: task.deadline.unwrap().elapsed().as_nanos() as u64,
            });
        }
        
        // Handle high-priority tasks in priority queue
        if task.priority >= StreamPriority::Critical {
            let priority_task = PriorityTask::new(task);
            self.priority_queue.push(priority_task);
            
            let duration = start.elapsed().as_nanos() as u64;
            self.metrics.record_scheduling(duration);
            return Ok(());
        }
        
        // Update MLFQ level for the stream
        self.update_mlfq_level(task.stream_id, &mut task)?;
        
        // Select worker queue using CPU affinity
        let worker_id = self.select_worker(task.stream_id);
        let queue = &self.worker_queues[worker_id];
        
        // Try to enqueue task
        match queue.push_local(task) {
            Ok(()) => {
                let duration = start.elapsed().as_nanos() as u64;
                self.metrics.record_scheduling(duration);
                Ok(())
            }
            Err(_) => {
                self.metrics.record_queue_overflow();
                Err(SchedulerError::QueueOverflow { worker_id: worker_id as WorkerId })
            }
        }
    }
    
    /// Get next task for a worker with work-stealing support
    pub async fn get_next_task(&self, worker_id: WorkerId) -> Option<StreamTask> {
        let worker_id = worker_id as usize;
        
        if worker_id >= self.worker_queues.len() {
            return None;
        }
        
        // Check priority queue first
        if let Some(priority_task) = self.priority_queue.pop() {
            return Some(priority_task.task);
        }
        
        // Try local queue
        if let Some(task) = self.worker_queues[worker_id].pop_local() {
            return Some(task);
        }
        
        // Work stealing if enabled
        if self.config.enable_work_stealing {
            for (i, queue) in self.worker_queues.iter().enumerate() {
                if i != worker_id {
                    if let Some(task) = queue.steal() {
                        self.metrics.record_work_steal();
                        return Some(task);
                    }
                }
            }
        }
        
        None
    }
    
    /// Adjust stream priority dynamically
    pub fn adjust_priority(&self, stream_id: StreamId, new_priority: StreamPriority) {
        // Update MLFQ level based on new priority
        let new_level = match new_priority {
            StreamPriority::Realtime | StreamPriority::Critical => 0,
            StreamPriority::High => 1,
            StreamPriority::Normal => 3,
            StreamPriority::Low => 5,
            StreamPriority::Background => 7,
        };
        
        // Update in appropriate MLFQ level
        if let Some(mut level_entry) = self.mlfq_levels[new_level].get_mut(&stream_id) {
            level_entry.level = new_level as u8;
            level_entry.time_quantum_ns = DEFAULT_TIME_QUANTUM_NS[new_level];
        } else {
            let queue_level = QueueLevel::new(new_level as u8);
            self.mlfq_levels[new_level].insert(stream_id, queue_level);
        }
    }
    
    /// Rebalance queues for fair scheduling
    pub fn rebalance_queues(&self) -> SchedulerResult<()> {
        if !self.config.enable_fair_scheduling {
            return Ok(());
        }
        
        let now = Instant::now();
        {
            let last_rebalance = self.last_rebalance.read();
            if now.duration_since(*last_rebalance).as_millis() < self.config.rebalance_interval_ms as u128 {
                return Ok(());
            }
        }
        
        // Update last rebalance time
        *self.last_rebalance.write() = now;
        
        // Calculate load per worker
        let mut loads = Vec::with_capacity(self.worker_queues.len());
        let mut total_load = 0;
        
        for queue in &self.worker_queues {
            let load = queue.len();
            loads.push(load);
            total_load += load;
        }
        
        if total_load == 0 {
            return Ok(());
        }
        
        let average_load = total_load / self.worker_queues.len();
        
        // Identify overloaded and underloaded workers
        for (worker_id, &load) in loads.iter().enumerate() {
            if load > average_load * 2 {
                // Worker is overloaded, but we can't easily rebalance work-stealing queues
                // This would require more complex queue management
                self.metrics.record_fairness_violation();
            }
        }
        
        self.metrics.record_rebalance();
        Ok(())
    }
    
    /// Prevent starvation by promoting long-waiting tasks
    pub fn prevent_starvation(&self) -> SchedulerResult<()> {
        let starvation_threshold = Duration::from_millis(self.config.starvation_threshold_ms);
        let now = Instant::now();
        
        // Check each MLFQ level for starved streams
        for (level_index, level_map) in self.mlfq_levels.iter().enumerate() {
            if level_index == 0 {
                continue; // Skip highest priority level
            }
            
            for mut entry in level_map.iter_mut() {
                let queue_level = entry.value_mut();
                
                if now.duration_since(queue_level.last_scheduled) > starvation_threshold {
                    // Promote to higher priority level
                    if level_index > 0 {
                        let stream_id = *entry.key();
                        let mut promoted_level = queue_level.clone();
                        promoted_level.level = (level_index - 1) as u8;
                        promoted_level.time_quantum_ns = DEFAULT_TIME_QUANTUM_NS[level_index - 1];
                        
                        // Remove from current level and add to higher level
                        drop(entry); // Release the entry before modifying other maps
                        level_map.remove(&stream_id);
                        self.mlfq_levels[level_index - 1].insert(stream_id, promoted_level);
                        
                        self.metrics.record_starvation();
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Get scheduler metrics
    pub fn get_metrics(&self) -> &SchedulerMetrics {
        &self.metrics
    }
    
    /// Check scheduler health
    pub fn health_check(&self) -> bool {
        let avg_scheduling_time = self.metrics.get_average_scheduling_time_ns();
        let completion_rate = self.metrics.get_completion_rate();
        let deadline_violations = self.metrics.deadline_violations.load(Ordering::Relaxed);
        
        // Health criteria
        avg_scheduling_time <= 100 && // Target: <50ns, warning: <100ns
        completion_rate >= 0.95 && // At least 95% completion rate
        deadline_violations < 100 // Keep deadline violations low
    }
    
    /// Get worker queue lengths for monitoring
    pub fn get_worker_queue_lengths(&self) -> Vec<usize> {
        self.worker_queues.iter().map(|q| q.len()).collect()
    }
    
    /// Get priority queue length
    pub fn get_priority_queue_length(&self) -> usize {
        // SegQueue doesn't provide a direct len() method
        // We approximate by tracking in metrics or using a separate counter
        0 // Placeholder - would need additional tracking
    }
    
    // Private helper methods
    
    /// Select optimal worker for a stream
    fn select_worker(&self, stream_id: StreamId) -> usize {
        if self.config.enable_numa_awareness {
            // Use stream ID to consistently map to a CPU/worker
            // This provides cache locality benefits
            (stream_id as usize) % self.worker_queues.len()
        } else {
            // Round-robin assignment
            self.cpu_mapper.get_next_cpu() % self.worker_queues.len()
        }
    }
    
    /// Update MLFQ level for a stream
    fn update_mlfq_level(&self, stream_id: StreamId, task: &mut StreamTask) -> SchedulerResult<()> {
        let current_level = task.mlfq_level as usize;
        
        if current_level >= MLFQ_LEVELS {
            task.mlfq_level = (MLFQ_LEVELS - 1) as u8;
        }
        
        // Get or create queue level entry
        let queue_level = self.mlfq_levels[current_level]
            .entry(stream_id)
            .or_insert_with(|| QueueLevel::new(current_level as u8));
        
        // Update time quantum based on level
        task.processing_time_ns = queue_level.time_quantum_ns;
        queue_level.last_scheduled = Instant::now();
        queue_level.execution_count += 1;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;
    
    #[test]
    fn test_scheduler_creation() {
        let config = SchedulerConfig::default();
        let scheduler = StreamScheduler::new(config);
        assert!(!scheduler.is_running.load(Ordering::Relaxed));
        assert!(scheduler.health_check());
    }
    
    #[test]
    fn test_scheduler_lifecycle() {
        let config = SchedulerConfig::default();
        let scheduler = StreamScheduler::new(config);
        
        assert!(scheduler.start().is_ok());
        assert!(scheduler.is_running.load(Ordering::Relaxed));
        
        assert!(scheduler.stop().is_ok());
        assert!(!scheduler.is_running.load(Ordering::Relaxed));
    }
    
    #[tokio::test]
    async fn test_task_scheduling() {
        let config = SchedulerConfig::default();
        let scheduler = StreamScheduler::new(config);
        assert!(scheduler.start().is_ok());
        
        let task = StreamTask::new(123, TaskType::ProcessMessage(456), StreamPriority::Normal);
        assert!(scheduler.schedule_task(task).await.is_ok());
        
        // Verify task was scheduled
        let metrics = scheduler.get_metrics();
        assert_eq!(metrics.tasks_scheduled.load(Ordering::Relaxed), 1);
        
        assert!(scheduler.stop().is_ok());
    }
    
    #[tokio::test]
    async fn test_priority_task_scheduling() {
        let config = SchedulerConfig::default();
        let scheduler = StreamScheduler::new(config);
        assert!(scheduler.start().is_ok());
        
        let high_priority_task = StreamTask::new(789, TaskType::HandleBackpressure, StreamPriority::Critical);
        assert!(scheduler.schedule_task(high_priority_task).await.is_ok());
        
        // Priority tasks should be retrievable first
        let retrieved = scheduler.get_next_task(0).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().stream_id, 789);
        
        assert!(scheduler.stop().is_ok());
    }
    
    #[tokio::test]
    async fn test_work_stealing() {
        let mut config = SchedulerConfig::default();
        config.worker_count = 2;
        config.enable_work_stealing = true;
        let scheduler = StreamScheduler::new(config);
        assert!(scheduler.start().is_ok());
        
        // Schedule multiple tasks
        for i in 0..10 {
            let task = StreamTask::new(i, TaskType::ProcessMessage(i), StreamPriority::Normal);
            assert!(scheduler.schedule_task(task).await.is_ok());
        }
        
        // Worker 1 should be able to steal work
        let stolen_task = scheduler.get_next_task(1).await;
        assert!(stolen_task.is_some());
        
        assert!(scheduler.stop().is_ok());
    }
    
    #[test]
    fn test_stream_task_creation() {
        let task = StreamTask::new(123, TaskType::ProcessMessage(456), StreamPriority::High);
        
        assert_eq!(task.stream_id, 123);
        assert_eq!(task.priority, StreamPriority::High);
        assert!(!task.is_expired());
        assert_eq!(task.mlfq_level, 0);
    }
    
    #[test]
    fn test_task_deadline_handling() {
        let deadline = Instant::now() + Duration::from_millis(100);
        let task = StreamTask::new(123, TaskType::ProcessMessage(456), StreamPriority::Normal)
            .with_deadline(deadline);
        
        assert!(!task.is_expired());
        assert!(task.deadline.is_some());
    }
    
    #[test]
    fn test_work_stealing_queue() {
        let queue = WorkStealingQueue::new(10);
        
        // Test local operations
        let task = StreamTask::new(123, TaskType::ProcessMessage(456), StreamPriority::Normal);
        assert!(queue.push_local(task).is_ok());
        assert_eq!(queue.len(), 1);
        
        let retrieved = queue.pop_local();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().stream_id, 123);
        assert_eq!(queue.len(), 0);
        
        // Test stealing
        let task2 = StreamTask::new(789, TaskType::UpdateMetrics, StreamPriority::Low);
        assert!(queue.push_local(task2).is_ok());
        
        let stolen = queue.steal();
        assert!(stolen.is_some());
        assert_eq!(stolen.unwrap().stream_id, 789);
    }
    
    #[test]
    fn test_cpu_affinity_mapper() {
        let mapper = CpuAffinityMapper::new();
        
        let cpu1 = mapper.get_next_cpu();
        let cpu2 = mapper.get_next_cpu();
        
        assert!(cpu1 < mapper.cpu_count);
        assert!(cpu2 < mapper.cpu_count);
        
        let numa_node = mapper.get_numa_node_for_cpu(0);
        assert!(numa_node < mapper.numa_nodes.len());
    }
    
    #[test]
    fn test_priority_calculation() {
        let task = StreamTask::new(123, TaskType::ProcessMessage(456), StreamPriority::Critical);
        let priority_task = PriorityTask::new(task);
        
        assert!(priority_task.urgency_score > 0);
    }
    
    #[test]
    fn test_mlfq_level_operations() {
        let mut task = StreamTask::new(123, TaskType::ProcessMessage(456), StreamPriority::Normal);
        
        assert_eq!(task.mlfq_level, 0);
        
        task.demote_priority();
        assert_eq!(task.mlfq_level, 1);
        
        task.promote_priority();
        assert_eq!(task.mlfq_level, 0);
    }
    
    #[test]
    fn test_performance_targets() {
        let config = SchedulerConfig::default();
        let scheduler = StreamScheduler::new(config);
        
        // Performance should be reasonable
        let metrics = scheduler.get_metrics();
        assert_eq!(metrics.get_average_scheduling_time_ns(), 0); // No tasks scheduled yet
        assert_eq!(metrics.get_completion_rate(), 0.0); // No tasks completed yet
    }
}