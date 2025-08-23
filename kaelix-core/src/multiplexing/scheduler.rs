//! # Multi-Level Feedback Queue (MLFQ) Scheduler
//!
//! Ultra-high-performance task scheduler with adaptive stream prioritization
//! optimized for millisecond-precision task management and fairness.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::multiplexing::{Priority, StreamId};
use crate::{Error, Result};

/// Unique task identifier
pub type TaskId = u64;

/// MLFQ Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Number of priority levels
    pub priority_levels: usize,
    /// Time slice per priority level (in milliseconds)
    pub time_slices: Vec<u64>,
    /// Aging threshold (promote tasks after this many cycles)
    pub aging_threshold: u64,
    /// Maximum tasks per queue
    pub max_tasks_per_queue: usize,
    /// Enable work stealing
    pub enable_work_stealing: bool,
    /// Work stealing threshold
    pub work_steal_threshold: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            priority_levels: 5,
            time_slices: vec![10, 20, 40, 80, 160], // Exponential time slices
            aging_threshold: 100,
            max_tasks_per_queue: 10000,
            enable_work_stealing: true,
            work_steal_threshold: 10,
        }
    }
}

/// Task that can be scheduled
#[derive(Debug, Clone)]
pub struct SchedulableTask {
    pub task_id: TaskId,
    pub stream_id: StreamId,
    pub priority: Priority,
    pub created_at: Instant,
    pub last_executed: Option<Instant>,
    pub execution_count: u64,
    pub remaining_time_slice: Duration,
    pub total_runtime: Duration,
}

impl SchedulableTask {
    pub fn new(task_id: TaskId, stream_id: StreamId, priority: Priority) -> Self {
        let time_slice = Duration::from_millis(match priority {
            0..=50 => 10,   // High priority: short slice
            51..=150 => 20, // Medium priority: medium slice
            _ => 40,        // Low priority: long slice
        });

        Self {
            task_id,
            stream_id,
            priority,
            created_at: Instant::now(),
            last_executed: None,
            execution_count: 0,
            remaining_time_slice: time_slice,
            total_runtime: Duration::ZERO,
        }
    }

    /// Updates task after execution
    pub fn update_after_execution(&mut self, execution_time: Duration) {
        self.last_executed = Some(Instant::now());
        self.execution_count += 1;
        self.total_runtime += execution_time;
        self.remaining_time_slice = self.remaining_time_slice.saturating_sub(execution_time);
    }

    /// Checks if task needs to be demoted to lower priority queue
    pub fn needs_demotion(&self) -> bool {
        self.remaining_time_slice.is_zero()
    }

    /// Resets time slice for new priority level
    pub fn reset_time_slice(&mut self, time_slice: Duration) {
        self.remaining_time_slice = time_slice;
    }
}

/// Multi-Level Feedback Queue Scheduler
pub struct MLFQScheduler {
    /// Priority queues (index 0 = highest priority)
    queues: Vec<Arc<RwLock<VecDeque<SchedulableTask>>>>,

    /// Task lookup table
    task_lookup: DashMap<TaskId, usize>, // TaskId -> queue level

    /// Configuration
    config: SchedulerConfig,

    /// Statistics
    stats: SchedulerStats,

    /// Current round-robin queue index
    current_queue: Arc<RwLock<usize>>,

    /// Last aging check
    last_aging: Arc<RwLock<Instant>>,
}

impl std::fmt::Debug for MLFQScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MLFQScheduler")
            .field("config", &self.config)
            .field("stats", &self.stats)
            .field("queue_count", &self.queues.len())
            .field("total_tasks", &self.task_lookup.len())
            .finish()
    }
}

impl MLFQScheduler {
    /// Creates a new MLFQ scheduler
    pub fn new(config: SchedulerConfig) -> Self {
        let queue_count = config.priority_levels;
        let mut queues = Vec::with_capacity(queue_count);

        for _ in 0..queue_count {
            queues.push(Arc::new(RwLock::new(VecDeque::new())));
        }

        Self {
            queues,
            task_lookup: DashMap::new(),
            config,
            stats: SchedulerStats::new(),
            current_queue: Arc::new(RwLock::new(0)),
            last_aging: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Adds a task to the scheduler
    pub async fn add_task(&self, task: SchedulableTask) -> Result<()> {
        let queue_level = self.priority_to_queue_level(task.priority);

        // Check capacity
        {
            let queue = self.queues[queue_level].read().await;
            if queue.len() >= self.config.max_tasks_per_queue {
                return Err(Error::ResourceExhausted(format!(
                    "Queue level {} is at capacity",
                    queue_level
                )));
            }
        }

        // Add to appropriate queue
        {
            let mut queue = self.queues[queue_level].write().await;
            queue.push_back(task.clone());
        }

        // Update lookup table
        self.task_lookup.insert(task.task_id, queue_level);

        // Update stats
        self.stats.tasks_added.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }

    /// Removes a task from the scheduler
    pub async fn remove_task(&self, task_id: TaskId) -> Result<()> {
        if let Some((_, queue_level)) = self.task_lookup.remove(&task_id) {
            let mut queue = self.queues[queue_level].write().await;
            queue.retain(|task| task.task_id != task_id);

            self.stats.tasks_removed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        } else {
            Err(Error::NotFound(format!("Task {} not found", task_id)))
        }
    }

    /// Gets the next task to execute (round-robin across queues)
    pub async fn get_next_task(&self) -> Option<SchedulableTask> {
        let queue_count = self.queues.len();

        // Check queues in priority order
        for _ in 0..queue_count {
            let current_index = {
                let mut current = self.current_queue.write().await;
                let index = *current;
                *current = (index + 1) % queue_count;
                index
            };

            let mut queue = self.queues[current_index].write().await;
            if let Some(task) = queue.pop_front() {
                self.stats.tasks_scheduled.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Some(task);
            }
        }

        None
    }

    /// Updates a task after execution and re-queues or demotes it
    pub async fn update_task_after_execution(
        &self,
        mut task: SchedulableTask,
        execution_time: Duration,
    ) -> Result<()> {
        task.update_after_execution(execution_time);

        if task.needs_demotion() {
            // Demote to lower priority queue
            let current_level =
                self.task_lookup.get(&task.task_id).map(|entry| *entry.value()).unwrap_or(0);

            let new_level = std::cmp::min(current_level + 1, self.config.priority_levels - 1);

            // Reset time slice for new level
            if new_level < self.config.time_slices.len() {
                let time_slice = Duration::from_millis(self.config.time_slices[new_level]);
                task.reset_time_slice(time_slice);
            }

            // Move to new queue
            {
                let mut queue = self.queues[new_level].write().await;
                queue.push_back(task.clone());
            }

            // Update lookup
            self.task_lookup.insert(task.task_id, new_level);

            self.stats.task_demotions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            // Re-queue at same level
            let queue_level =
                self.task_lookup.get(&task.task_id).map(|entry| *entry.value()).unwrap_or(0);

            let mut queue = self.queues[queue_level].write().await;
            queue.push_back(task);
        }

        Ok(())
    }

    /// Performs aging - promotes long-waiting tasks to higher priority queues
    pub async fn perform_aging(&self) -> Result<()> {
        let now = Instant::now();
        let mut last_aging = self.last_aging.write().await;

        if now.duration_since(*last_aging) < Duration::from_secs(1) {
            return Ok(()); // Don't age too frequently
        }

        *last_aging = now;
        drop(last_aging);

        let aging_threshold = Duration::from_millis(self.config.aging_threshold);
        let mut promoted_tasks = Vec::new();

        // Check lower priority queues for tasks that should be promoted
        for (queue_level, queue_arc) in self.queues.iter().enumerate().skip(1) {
            let mut queue = queue_arc.write().await;
            let mut remaining_tasks = VecDeque::new();

            while let Some(task) = queue.pop_front() {
                let wait_time = now.duration_since(task.last_executed.unwrap_or(task.created_at));

                if wait_time > aging_threshold {
                    // Promote this task
                    promoted_tasks.push((task, queue_level - 1));
                    self.stats.task_promotions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                } else {
                    remaining_tasks.push_back(task);
                }
            }

            *queue = remaining_tasks;
        }

        // Add promoted tasks to higher priority queues
        for (mut task, new_level) in promoted_tasks {
            if new_level < self.config.time_slices.len() {
                let time_slice = Duration::from_millis(self.config.time_slices[new_level]);
                task.reset_time_slice(time_slice);
            }

            let mut queue = self.queues[new_level].write().await;
            queue.push_back(task.clone());
            self.task_lookup.insert(task.task_id, new_level);
        }

        Ok(())
    }

    /// Work stealing - moves tasks from overloaded queues to underloaded ones
    pub async fn perform_work_stealing(&self) -> Result<()> {
        if !self.config.enable_work_stealing {
            return Ok(());
        }

        let threshold = self.config.work_steal_threshold;
        let mut queue_sizes = Vec::new();

        // Collect queue sizes
        for queue_arc in &self.queues {
            let queue = queue_arc.read().await;
            queue_sizes.push(queue.len());
        }

        // Find overloaded and underloaded queues
        for (i, &size) in queue_sizes.iter().enumerate() {
            if size > threshold {
                // This queue is overloaded, try to steal from it
                for (j, &other_size) in queue_sizes.iter().enumerate() {
                    if i != j && other_size < threshold / 2 {
                        // Steal a task from queue i to queue j
                        if let Some(task) = {
                            let mut source_queue = self.queues[i].write().await;
                            source_queue.pop_back()
                        } {
                            let mut dest_queue = self.queues[j].write().await;
                            dest_queue.push_back(task.clone());
                            self.task_lookup.insert(task.task_id, j);

                            self.stats
                                .work_steals
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Gets current scheduler statistics
    pub fn get_stats(&self) -> &SchedulerStats {
        &self.stats
    }

    /// Gets the number of tasks in each queue
    pub async fn get_queue_sizes(&self) -> Vec<usize> {
        let mut sizes = Vec::with_capacity(self.queues.len());

        for queue_arc in &self.queues {
            let queue = queue_arc.read().await;
            sizes.push(queue.len());
        }

        sizes
    }

    /// Converts priority to queue level
    fn priority_to_queue_level(&self, priority: Priority) -> usize {
        let level = (priority as f64 / 255.0 * self.config.priority_levels as f64) as usize;
        std::cmp::min(level, self.config.priority_levels - 1)
    }
}

/// Scheduler statistics
#[derive(Debug, Default)]
pub struct SchedulerStats {
    pub tasks_added: std::sync::atomic::AtomicU64,
    pub tasks_removed: std::sync::atomic::AtomicU64,
    pub tasks_scheduled: std::sync::atomic::AtomicU64,
    pub task_promotions: std::sync::atomic::AtomicU64,
    pub task_demotions: std::sync::atomic::AtomicU64,
    pub work_steals: std::sync::atomic::AtomicU64,
}

impl SchedulerStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets snapshot of current statistics
    pub fn snapshot(&self) -> SchedulerStatsSnapshot {
        SchedulerStatsSnapshot {
            tasks_added: self.tasks_added.load(std::sync::atomic::Ordering::Relaxed),
            tasks_removed: self.tasks_removed.load(std::sync::atomic::Ordering::Relaxed),
            tasks_scheduled: self.tasks_scheduled.load(std::sync::atomic::Ordering::Relaxed),
            task_promotions: self.task_promotions.load(std::sync::atomic::Ordering::Relaxed),
            task_demotions: self.task_demotions.load(std::sync::atomic::Ordering::Relaxed),
            work_steals: self.work_steals.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// Immutable snapshot of scheduler statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStatsSnapshot {
    pub tasks_added: u64,
    pub tasks_removed: u64,
    pub tasks_scheduled: u64,
    pub task_promotions: u64,
    pub task_demotions: u64,
    pub work_steals: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler_creation() {
        let config = SchedulerConfig::default();
        let scheduler = MLFQScheduler::new(config);

        assert_eq!(scheduler.queues.len(), 5);
        assert_eq!(scheduler.task_lookup.len(), 0);
    }

    #[tokio::test]
    async fn test_task_addition() {
        let config = SchedulerConfig::default();
        let scheduler = MLFQScheduler::new(config);

        let task = SchedulableTask::new(1, 100, 50); // Medium priority
        scheduler.add_task(task).await.unwrap();

        assert_eq!(scheduler.task_lookup.len(), 1);
    }

    #[tokio::test]
    async fn test_task_scheduling() {
        let config = SchedulerConfig::default();
        let scheduler = MLFQScheduler::new(config);

        // Add a high priority task
        let task = SchedulableTask::new(1, 100, 10); // High priority
        scheduler.add_task(task).await.unwrap();

        // Schedule the task
        let scheduled_task = scheduler.get_next_task().await;
        assert!(scheduled_task.is_some());
        assert_eq!(scheduled_task.unwrap().task_id, 1);
    }

    #[tokio::test]
    async fn test_task_demotion() {
        let config = SchedulerConfig::default();
        let scheduler = MLFQScheduler::new(config);

        let mut task = SchedulableTask::new(1, 100, 50);
        scheduler.add_task(task.clone()).await.unwrap();

        // Simulate task execution that uses up time slice
        task.remaining_time_slice = Duration::ZERO;

        scheduler
            .update_task_after_execution(task, Duration::from_millis(20))
            .await
            .unwrap();

        let stats = scheduler.get_stats().snapshot();
        assert!(stats.task_demotions > 0);
    }

    #[tokio::test]
    async fn test_aging() {
        let config = SchedulerConfig::default();
        let scheduler = MLFQScheduler::new(config);

        // Add a task to a lower priority queue
        let task = SchedulableTask::new(1, 100, 200); // Low priority
        scheduler.add_task(task).await.unwrap();

        // Perform aging (this is a simplified test)
        scheduler.perform_aging().await.unwrap();

        // In a real test, we'd need to wait and check promotion
        // For now, just verify the method doesn't error
    }

    #[test]
    fn test_priority_to_queue_mapping() {
        let config = SchedulerConfig::default();
        let scheduler = MLFQScheduler::new(config);

        assert_eq!(scheduler.priority_to_queue_level(0), 0); // Highest priority -> Queue 0
        assert_eq!(scheduler.priority_to_queue_level(255), 4); // Lowest priority -> Queue 4
        assert_eq!(scheduler.priority_to_queue_level(127), 2); // Medium priority -> Queue 2
    }

    #[tokio::test]
    async fn test_task_removal() {
        let config = SchedulerConfig::default();
        let scheduler = MLFQScheduler::new(config);

        let task = SchedulableTask::new(1, 100, 50);
        scheduler.add_task(task).await.unwrap();

        assert_eq!(scheduler.task_lookup.len(), 1);

        scheduler.remove_task(1).await.unwrap();

        assert_eq!(scheduler.task_lookup.len(), 0);
    }
}
