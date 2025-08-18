//! High-Performance Task Scheduler
//!
//! Provides NUMA-aware task scheduling with work-stealing capabilities for optimal
//! load distribution and minimal context switching overhead.

use crate::runtime::{
    affinity::{NumaTopology, NodeId},
    executor::{RuntimeConfig, TaskId},
    worker::WorkerId,
};
use futures::future::BoxFuture;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::Waker,
};
use crossbeam::{
    deque::Injector,
    queue::SegQueue,
};
use parking_lot::RwLock;

/// Task representation for the scheduler
/// 
/// Note: We use Arc to make the Task Send + Sync for cross-thread work stealing
pub struct Task {
    /// Unique task identifier
    pub id: TaskId,
    
    /// Future to execute - wrapped in Arc for thread safety
    pub future: Arc<parking_lot::Mutex<Option<BoxFuture<'static, ()>>>>,
    
    /// Task priority (higher = more important)
    pub priority: TaskPriority,
    
    /// Preferred NUMA node for execution
    pub preferred_node: Option<NodeId>,
    
    /// Task creation timestamp for latency tracking
    pub created_at: std::time::Instant,
    
    /// Task waker for notifications
    pub waker: Option<Waker>,
}

unsafe impl Send for Task {}
unsafe impl Sync for Task {}

impl Task {
    /// Create a new task
    pub fn new(id: TaskId, future: BoxFuture<'static, ()>) -> Self {
        Self {
            id,
            future: Arc::new(parking_lot::Mutex::new(Some(future))),
            priority: TaskPriority::Normal,
            preferred_node: None,
            created_at: std::time::Instant::now(),
            waker: None,
        }
    }
    
    /// Create a task with specific priority
    pub fn with_priority(id: TaskId, future: BoxFuture<'static, ()>, priority: TaskPriority) -> Self {
        Self {
            id,
            future: Arc::new(parking_lot::Mutex::new(Some(future))),
            priority,
            preferred_node: None,
            created_at: std::time::Instant::now(),
            waker: None,
        }
    }
    
    /// Create a task with NUMA node preference
    pub fn with_numa_preference(
        id: TaskId,
        future: BoxFuture<'static, ()>,
        node: NodeId,
    ) -> Self {
        Self {
            id,
            future: Arc::new(parking_lot::Mutex::new(Some(future))),
            priority: TaskPriority::Normal,
            preferred_node: Some(node),
            created_at: std::time::Instant::now(),
            waker: None,
        }
    }
    
    /// Take the future for execution (can only be called once)
    pub fn take_future(&mut self) -> Option<BoxFuture<'static, ()>> {
        self.future.lock().take()
    }
    
    /// Get the task age since creation
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
    
    /// Check if this task should be executed before another
    pub fn should_execute_before(&self, other: &Task) -> bool {
        match self.priority.cmp(&other.priority) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => {
                // Same priority: older tasks first (FIFO)
                self.created_at < other.created_at
            }
        }
    }
}

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    /// Low priority background tasks
    Low = 0,
    
    /// Normal priority tasks (default)
    Normal = 1,
    
    /// High priority latency-sensitive tasks
    High = 2,
    
    /// Critical real-time tasks
    Critical = 3,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

/// Lock-free task queues with work-stealing support
/// 
/// Simplified implementation using SegQueue for thread safety
#[derive(Clone)]
pub struct TaskQueues {
    /// Global task injector (for new tasks)
    global_queue: Arc<Injector<Task>>,
    
    /// Per-worker local queues using SegQueue
    local_queues: Vec<Arc<SegQueue<Task>>>,
    
    /// High-priority task queue
    priority_queue: Arc<SegQueue<Task>>,
    
    /// Queue statistics
    stats: Arc<QueueStats>,
}

impl TaskQueues {
    /// Create new task queues for the given number of workers
    pub fn new(worker_count: usize, _queue_depth: usize) -> Self {
        let mut local_queues = Vec::with_capacity(worker_count);
        
        // Create local queues using SegQueue for simplicity
        for _ in 0..worker_count {
            local_queues.push(Arc::new(SegQueue::new()));
        }
        
        Self {
            global_queue: Arc::new(Injector::new()),
            local_queues,
            priority_queue: Arc::new(SegQueue::new()),
            stats: Arc::new(QueueStats::new()),
        }
    }
    
    /// Get the local queue for a specific worker
    pub fn local_queue(&self, worker_id: usize) -> Arc<SegQueue<Task>> {
        self.local_queues[worker_id].clone()
    }
    
    /// Get all local queues for work-stealing
    pub fn all_local_queues(&self) -> &[Arc<SegQueue<Task>>] {
        &self.local_queues
    }
    
    /// Push a task to the global queue
    pub fn push_global(&self, task: Task) {
        if task.priority >= TaskPriority::High {
            self.priority_queue.push(task);
            self.stats.priority_enqueued.fetch_add(1, Ordering::Relaxed);
        } else {
            self.global_queue.push(task);
            self.stats.global_enqueued.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Push a task to a specific worker's local queue
    pub fn push_local(&self, worker_id: usize, task: Task) {
        if worker_id < self.local_queues.len() {
            self.local_queues[worker_id].push(task);
            self.stats.local_enqueued.fetch_add(1, Ordering::Relaxed);
        } else {
            // Fallback to global queue
            self.push_global(task);
        }
    }
    
    /// Try to pop a high-priority task
    pub fn try_pop_priority(&self) -> Option<Task> {
        if let Some(task) = self.priority_queue.pop() {
            self.stats.priority_dequeued.fetch_add(1, Ordering::Relaxed);
            Some(task)
        } else {
            None
        }
    }
    
    /// Try to pop a task from the global queue
    pub fn try_pop_global(&self) -> Option<Task> {
        if let Some(task) = self.global_queue.steal().success() {
            self.stats.global_dequeued.fetch_add(1, Ordering::Relaxed);
            Some(task)
        } else {
            None
        }
    }
    
    /// Try to pop from a specific local queue
    pub fn try_pop_local(&self, worker_id: usize) -> Option<Task> {
        if worker_id < self.local_queues.len() {
            if let Some(task) = self.local_queues[worker_id].pop() {
                self.stats.local_dequeued.fetch_add(1, Ordering::Relaxed);
                Some(task)
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// Get queue statistics
    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }
    
    /// Drain all tasks from all queues (used during shutdown)
    pub async fn drain_all(&self) {
        // Drain priority queue
        while self.priority_queue.pop().is_some() {}
        
        // Drain global queue
        while self.global_queue.steal().success().is_some() {}
        
        // Drain local queues
        for local_queue in &self.local_queues {
            while local_queue.pop().is_some() {}
        }
    }
}

/// Queue operation statistics
pub struct QueueStats {
    pub global_enqueued: AtomicUsize,
    pub global_dequeued: AtomicUsize,
    pub local_enqueued: AtomicUsize,
    pub local_dequeued: AtomicUsize,
    pub priority_enqueued: AtomicUsize,
    pub priority_dequeued: AtomicUsize,
    pub steals_attempted: AtomicUsize,
    pub steals_successful: AtomicUsize,
}

impl QueueStats {
    pub fn new() -> Self {
        Self {
            global_enqueued: AtomicUsize::new(0),
            global_dequeued: AtomicUsize::new(0),
            local_enqueued: AtomicUsize::new(0),
            local_dequeued: AtomicUsize::new(0),
            priority_enqueued: AtomicUsize::new(0),
            priority_dequeued: AtomicUsize::new(0),
            steals_attempted: AtomicUsize::new(0),
            steals_successful: AtomicUsize::new(0),
        }
    }
    
    /// Get the number of pending tasks across all queues
    pub fn pending_tasks(&self) -> usize {
        let global_pending = self.global_enqueued.load(Ordering::Relaxed)
            .saturating_sub(self.global_dequeued.load(Ordering::Relaxed));
        let local_pending = self.local_enqueued.load(Ordering::Relaxed)
            .saturating_sub(self.local_dequeued.load(Ordering::Relaxed));
        let priority_pending = self.priority_enqueued.load(Ordering::Relaxed)
            .saturating_sub(self.priority_dequeued.load(Ordering::Relaxed));
        
        global_pending + local_pending + priority_pending
    }
    
    /// Get work-stealing success rate
    pub fn steal_success_rate(&self) -> f64 {
        let attempted = self.steals_attempted.load(Ordering::Relaxed);
        if attempted == 0 {
            0.0
        } else {
            let successful = self.steals_successful.load(Ordering::Relaxed);
            successful as f64 / attempted as f64
        }
    }
}

/// NUMA-aware task scheduler
pub struct TaskScheduler {
    /// Task queues for work distribution
    queues: TaskQueues,
    
    /// NUMA topology information
    numa_topology: Arc<NumaTopology>,
    
    /// Worker to NUMA node mapping
    worker_numa_mapping: HashMap<WorkerId, NodeId>,
    
    /// Load balancing statistics per NUMA node
    node_load: Arc<RwLock<HashMap<NodeId, NodeLoad>>>,
    
    /// Scheduling policy configuration
    policy: SchedulingPolicy,
    
    /// Next worker for round-robin scheduling
    next_worker: AtomicUsize,
}

/// Load information for a NUMA node
#[derive(Debug, Clone)]
struct NodeLoad {
    /// Number of tasks currently assigned to this node
    task_count: usize,
    
    /// Average task execution time on this node
    avg_execution_time: std::time::Duration,
    
    /// Worker utilization on this node
    utilization: f64,
}

impl Default for NodeLoad {
    fn default() -> Self {
        Self {
            task_count: 0,
            avg_execution_time: std::time::Duration::from_micros(1),
            utilization: 0.0,
        }
    }
}

/// Scheduling policy configuration
#[derive(Debug, Clone)]
pub struct SchedulingPolicy {
    /// Enable NUMA-aware scheduling
    pub numa_aware: bool,
    
    /// Enable work stealing between workers
    pub work_stealing: bool,
    
    /// Load balancing threshold (rebalance when difference > threshold)
    pub load_balance_threshold: f64,
    
    /// Maximum steal attempts per worker poll
    pub max_steal_attempts: usize,
    
    /// Prefer local NUMA node for task execution
    pub prefer_local_numa: bool,
}

impl Default for SchedulingPolicy {
    fn default() -> Self {
        Self {
            numa_aware: true,
            work_stealing: true,
            load_balance_threshold: 0.2, // 20% difference triggers rebalancing
            max_steal_attempts: 3,
            prefer_local_numa: true,
        }
    }
}

impl TaskScheduler {
    /// Create a new task scheduler
    pub fn new(
        numa_topology: &NumaTopology,
        config: &RuntimeConfig,
        queues: TaskQueues,
    ) -> Self {
        let numa_topology = Arc::new(numa_topology.clone());
        
        // Build worker to NUMA node mapping
        let worker_count = queues.local_queues.len();
        let placements = numa_topology.optimal_worker_placement(worker_count);
        
        let mut worker_numa_mapping = HashMap::new();
        for (worker_idx, cpu_set) in placements.iter().enumerate() {
            if let Some(numa_node) = cpu_set.numa_node() {
                worker_numa_mapping.insert(WorkerId::new(worker_idx), numa_node);
            }
        }
        
        // Initialize node load tracking
        let mut node_load = HashMap::new();
        for node in numa_topology.nodes() {
            node_load.insert(node.id, NodeLoad::default());
        }
        
        let policy = SchedulingPolicy {
            numa_aware: config.numa_awareness,
            work_stealing: config.work_stealing,
            load_balance_threshold: 0.2,
            max_steal_attempts: 3,
            prefer_local_numa: true,
        };
        
        Self {
            queues,
            numa_topology,
            worker_numa_mapping,
            node_load: Arc::new(RwLock::new(node_load)),
            policy,
            next_worker: AtomicUsize::new(0),
        }
    }
    
    /// Schedule a task for execution
    pub fn schedule_task(&self, future: BoxFuture<'static, ()>, task_id: TaskId) {
        let task = Task::new(task_id, future);
        
        if self.policy.numa_aware {
            self.schedule_numa_aware(task);
        } else {
            self.schedule_round_robin(task);
        }
    }
    
    /// Schedule a task with specific priority
    pub fn schedule_task_with_priority(
        &self,
        future: BoxFuture<'static, ()>,
        task_id: TaskId,
        priority: TaskPriority,
    ) {
        let task = Task::with_priority(task_id, future, priority);
        
        if task.priority >= TaskPriority::High {
            // High-priority tasks go to the priority queue
            self.queues.push_global(task);
        } else if self.policy.numa_aware {
            self.schedule_numa_aware(task);
        } else {
            self.schedule_round_robin(task);
        }
    }
    
    /// Schedule a task with NUMA node preference
    pub fn schedule_task_with_numa_preference(
        &self,
        future: BoxFuture<'static, ()>,
        task_id: TaskId,
        preferred_node: NodeId,
    ) {
        let task = Task::with_numa_preference(task_id, future, preferred_node);
        self.schedule_numa_aware(task);
    }
    
    /// NUMA-aware task scheduling
    fn schedule_numa_aware(&self, mut task: Task) {
        let target_node = if let Some(preferred) = task.preferred_node {
            preferred
        } else {
            // Select least loaded NUMA node
            self.select_least_loaded_node()
        };
        
        task.preferred_node = Some(target_node);
        
        // Find a worker on the target NUMA node
        if let Some(worker_id) = self.find_worker_on_node(target_node) {
            self.queues.push_local(worker_id.id(), task);
            
            // Update load tracking
            self.update_node_load(target_node, 1);
        } else {
            // Fallback to global queue
            self.queues.push_global(task);
        }
    }
    
    /// Round-robin task scheduling
    fn schedule_round_robin(&self, task: Task) {
        let worker_count = self.queues.local_queues.len();
        let worker_id = self.next_worker.fetch_add(1, Ordering::Relaxed) % worker_count;
        self.queues.push_local(worker_id, task);
    }
    
    /// Select the least loaded NUMA node
    fn select_least_loaded_node(&self) -> NodeId {
        let node_load = self.node_load.read();
        
        if let Some(numa_nodes) = self.numa_topology.nodes().get(0) {
            let mut least_loaded_node = numa_nodes.id;
            let mut min_load = f64::MAX;
            
            for node in self.numa_topology.nodes() {
                if let Some(load) = node_load.get(&node.id) {
                    let current_load = load.utilization + (load.task_count as f64 * 0.1);
                    if current_load < min_load {
                        min_load = current_load;
                        least_loaded_node = node.id;
                    }
                }
            }
            
            least_loaded_node
        } else {
            NodeId(0) // Fallback to node 0
        }
    }
    
    /// Find a worker on a specific NUMA node
    fn find_worker_on_node(&self, node_id: NodeId) -> Option<WorkerId> {
        for (&worker_id, &worker_node) in &self.worker_numa_mapping {
            if worker_node == node_id {
                return Some(worker_id);
            }
        }
        None
    }
    
    /// Update load tracking for a NUMA node
    fn update_node_load(&self, node_id: NodeId, task_delta: i32) {
        let mut node_load = self.node_load.write();
        if let Some(load) = node_load.get_mut(&node_id) {
            if task_delta > 0 {
                load.task_count = load.task_count.saturating_add(task_delta as usize);
            } else {
                load.task_count = load.task_count.saturating_sub((-task_delta) as usize);
            }
        }
    }
    
    /// Get task queues
    pub fn queues(&self) -> &TaskQueues {
        &self.queues
    }
    
    /// Get scheduling policy
    pub fn policy(&self) -> &SchedulingPolicy {
        &self.policy
    }
    
    /// Get current load distribution across NUMA nodes
    pub fn load_distribution(&self) -> HashMap<NodeId, NodeLoad> {
        self.node_load.read().clone()
    }
    
    /// Trigger load balancing across NUMA nodes
    pub fn rebalance_load(&self) {
        if !self.policy.numa_aware {
            return;
        }
        
        let node_load = self.node_load.read();
        let mut loads: Vec<_> = node_load.iter().collect();
        loads.sort_by(|a, b| a.1.utilization.partial_cmp(&b.1.utilization).unwrap());
        
        if loads.len() < 2 {
            return;
        }
        
        let min_load = loads[0].1.utilization;
        let max_load = loads[loads.len() - 1].1.utilization;
        
        if max_load - min_load > self.policy.load_balance_threshold {
            tracing::debug!(
                "Load imbalance detected: min={:.2}%, max={:.2}%, triggering rebalancing",
                min_load * 100.0,
                max_load * 100.0
            );
            
            // TODO: Implement actual task migration between nodes
            // This would involve moving tasks from overloaded to underloaded nodes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::affinity::NumaTopology;
    use futures::future::ready;

    #[test]
    fn test_task_creation() {
        let task_id = TaskId::new();
        let future = Box::pin(ready(()));
        let task = Task::new(task_id, future);
        
        assert_eq!(task.id, task_id);
        assert_eq!(task.priority, TaskPriority::Normal);
        assert!(task.preferred_node.is_none());
        assert!(task.age().as_nanos() > 0);
    }
    
    #[test]
    fn test_task_priority_ordering() {
        let task_id = TaskId::new();
        let future1 = Box::pin(ready(()));
        let future2 = Box::pin(ready(()));
        
        let high_task = Task::with_priority(task_id, future1, TaskPriority::High);
        let normal_task = Task::with_priority(task_id, future2, TaskPriority::Normal);
        
        assert!(high_task.should_execute_before(&normal_task));
        assert!(!normal_task.should_execute_before(&high_task));
    }
    
    #[test]
    fn test_task_queues_creation() {
        let queues = TaskQueues::new(4, 1024);
        assert_eq!(queues.local_queues.len(), 4);
    }
    
    #[test]
    fn test_task_queues_push_pop() {
        let queues = TaskQueues::new(2, 1024);
        let task_id = TaskId::new();
        let future = Box::pin(ready(()));
        let task = Task::new(task_id, future);
        
        // Test global queue
        queues.push_global(task);
        assert!(queues.try_pop_global().is_some());
        assert!(queues.try_pop_global().is_none());
    }
    
    #[test]
    fn test_priority_queue() {
        let queues = TaskQueues::new(2, 1024);
        let task_id = TaskId::new();
        let future = Box::pin(ready(()));
        let high_priority_task = Task::with_priority(task_id, future, TaskPriority::High);
        
        queues.push_global(high_priority_task);
        assert!(queues.try_pop_priority().is_some());
        assert!(queues.try_pop_priority().is_none());
    }
    
    #[test]
    fn test_queue_stats() {
        let queues = TaskQueues::new(2, 1024);
        let stats = queues.stats();
        
        assert_eq!(stats.pending_tasks(), 0);
        assert_eq!(stats.steal_success_rate(), 0.0);
        
        // Add some tasks and verify stats
        let task_id = TaskId::new();
        let future = Box::pin(ready(()));
        let task = Task::new(task_id, future);
        queues.push_global(task);
        
        assert!(stats.pending_tasks() > 0);
    }
    
    #[tokio::test]
    async fn test_task_scheduler_creation() {
        if let Ok(numa_topology) = NumaTopology::detect() {
            let config = RuntimeConfig::default();
            let queues = TaskQueues::new(4, 1024);
            
            let scheduler = TaskScheduler::new(&numa_topology, &config, queues);
            
            assert!(scheduler.policy.numa_aware);
            assert!(scheduler.policy.work_stealing);
            
            // Test task scheduling
            let task_id = TaskId::new();
            let future = Box::pin(ready(()));
            scheduler.schedule_task(future, task_id);
            
            // Verify load distribution
            let load_dist = scheduler.load_distribution();
            assert!(!load_dist.is_empty());
        }
    }
    
    #[test]
    fn test_scheduling_policy() {
        let policy = SchedulingPolicy::default();
        assert!(policy.numa_aware);
        assert!(policy.work_stealing);
        assert_eq!(policy.load_balance_threshold, 0.2);
        assert_eq!(policy.max_steal_attempts, 3);
        assert!(policy.prefer_local_numa);
    }
}