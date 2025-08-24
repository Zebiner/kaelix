//! Runtime system for Kaelix message processing.

use crate::{error::Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinHandle};

pub mod affinity;
pub mod diagnostics;
pub mod executor;
pub mod metrics;
pub mod scheduler;
pub mod worker;

// Re-export performance types from telemetry
pub use crate::telemetry::performance;

/// Target task latency in microseconds for ultra-low-latency operations
pub const TARGET_TASK_LATENCY_US: u64 = 10;

/// Runtime-specific result type
pub type RuntimeResult<T> = std::result::Result<T, RuntimeError>;

/// Runtime-specific error types
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    #[error("CPU affinity error: {0}")]
    CpuAffinity(String),

    #[error("NUMA topology error: {0}")]
    NumaTopology(String),

    #[error("Worker thread error: {0}")]
    WorkerThread(String),

    #[error("Scheduler error: {0}")]
    Scheduler(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Number of worker threads
    pub worker_threads: usize,
    /// Task queue size
    pub task_queue_size: usize,
    /// Worker timeout
    pub worker_timeout: Duration,
    /// Enable runtime metrics
    pub enable_metrics: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_threads: num_cpus::get(),
            task_queue_size: 10000,
            worker_timeout: Duration::from_secs(30),
            enable_metrics: true,
        }
    }
}

/// Health status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Health monitor for tracking system health
#[derive(Debug, Default)]
pub struct HealthMonitor {
    status: Arc<std::sync::RwLock<HealthStatus>>,
    last_check: Arc<std::sync::RwLock<Option<Instant>>>,
    error_count: AtomicU64,
    total_checks: AtomicU64,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_status(&self) -> HealthStatus {
        *self.status.read().unwrap()
    }

    pub fn update_status(&self, status: HealthStatus) {
        *self.status.write().unwrap() = status;
        *self.last_check.write().unwrap() = Some(Instant::now());
        self.total_checks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);

        // Update status to degraded if error rate is high
        let total = self.total_checks.load(Ordering::Relaxed);
        let errors = self.error_count.load(Ordering::Relaxed);

        if total > 0 && (errors as f64 / total as f64) > 0.1 {
            self.update_status(HealthStatus::Degraded);
        }
    }

    pub fn get_error_rate(&self) -> f64 {
        let total = self.total_checks.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.error_count.load(Ordering::Relaxed) as f64 / total as f64
        }
    }
}

/// Optimized runtime for high-performance message processing
pub struct OptimizedRuntime {
    config: RuntimeConfig,
    scheduler: TaskScheduler,
    health_monitor: HealthMonitor,
    is_running: AtomicBool,
}

impl OptimizedRuntime {
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        let scheduler = TaskScheduler::new(config.clone())?;
        let health_monitor = HealthMonitor::new();

        Ok(Self { config, scheduler, health_monitor, is_running: AtomicBool::new(false) })
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.is_running.load(Ordering::Relaxed) {
            return Err(Error::AlreadyStarted);
        }

        self.scheduler.start()?;
        self.is_running.store(true, Ordering::Relaxed);
        self.health_monitor.update_status(HealthStatus::Healthy);

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        if !self.is_running.load(Ordering::Relaxed) {
            return Ok(());
        }

        self.scheduler.stop()?;
        self.is_running.store(false, Ordering::Relaxed);
        self.health_monitor.update_status(HealthStatus::Unknown);

        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    pub fn health_status(&self) -> HealthStatus {
        self.health_monitor.get_status()
    }

    pub fn schedule<F>(&self, _task: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if !self.is_running.load(Ordering::Relaxed) {
            return Err(Error::RuntimeNotStarted);
        }

        // Implementation would schedule the task
        Ok(())
    }
}

/// Task scheduler for managing runtime tasks
struct TaskScheduler {
    config: RuntimeConfig,
    sender: mpsc::UnboundedSender<Box<dyn std::future::Future<Output = ()> + Send>>,
    handles: Vec<JoinHandle<()>>,
}

impl TaskScheduler {
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        let (_sender, _receiver) = mpsc::unbounded_channel();

        Ok(Self { config, sender: _sender, handles: Vec::new() })
    }

    pub fn start(&mut self) -> Result<()> {
        // Implementation would start worker tasks
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        // Implementation would stop worker tasks
        Ok(())
    }
}

/// Global runtime shutdown function
pub async fn shutdown_runtime() -> Result<()> {
    // Implementation would shut down any global runtime state
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_creation() {
        let config = RuntimeConfig::default();
        let runtime = OptimizedRuntime::new(config);
        assert!(runtime.is_ok());
    }

    #[tokio::test]
    async fn test_runtime_lifecycle() {
        let config = RuntimeConfig::default();
        let mut runtime = OptimizedRuntime::new(config).unwrap();

        assert!(!runtime.is_running());
        assert_eq!(runtime.health_status(), HealthStatus::Unknown);

        runtime.start().await.unwrap();
        assert!(runtime.is_running());
        assert_eq!(runtime.health_status(), HealthStatus::Healthy);

        runtime.shutdown().await.unwrap();
        assert!(!runtime.is_running());
    }

    #[test]
    fn test_health_monitor() {
        let monitor = HealthMonitor::new();

        assert_eq!(monitor.get_status(), HealthStatus::Unknown);
        assert_eq!(monitor.get_error_rate(), 0.0);

        monitor.update_status(HealthStatus::Healthy);
        assert_eq!(monitor.get_status(), HealthStatus::Healthy);

        monitor.record_error();
        assert!(monitor.get_error_rate() > 0.0);
    }
}
