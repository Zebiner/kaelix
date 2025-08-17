//! Process orchestration and resource management for distributed testing
//!
//! This module provides infrastructure for managing test processes, enforcing
//! resource constraints, and collecting distributed logs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::cluster::{NodeId, ResourceConstraints};

/// Process manager for orchestrating test processes
pub struct ProcessManager {
    /// Active processes
    processes: Arc<RwLock<HashMap<ProcessId, ManagedProcess>>>,
    /// Resource monitor
    resource_monitor: Arc<ResourceMonitor>,
    /// Log collector
    log_collector: Arc<LogCollector>,
    /// Manager state
    state: Arc<Mutex<ManagerState>>,
}

/// Unique identifier for managed processes
pub type ProcessId = Uuid;

/// A managed process with resource constraints and monitoring
pub struct ManagedProcess {
    /// Process identifier
    pub id: ProcessId,
    /// Process configuration
    pub config: ProcessConfig,
    /// Underlying process handle
    pub handle: Child,
    /// Resource constraints
    pub constraints: ResourceConstraints,
    /// Process state
    state: Arc<Mutex<ProcessState>>,
    /// Start time
    started_at: Instant,
    /// Resource usage tracker
    resource_usage: Arc<Mutex<ResourceUsage>>,
}

/// Configuration for a managed process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    /// Process identifier
    pub id: ProcessId,
    /// Command to execute
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Working directory
    pub working_dir: Option<PathBuf>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Standard input handling
    pub stdin: StdioConfig,
    /// Standard output handling
    pub stdout: StdioConfig,
    /// Standard error handling
    pub stderr: StdioConfig,
    /// Process timeout
    pub timeout: Option<Duration>,
    /// Auto-restart on failure
    pub auto_restart: bool,
    /// Health check configuration
    pub health_check: Option<HealthCheckConfig>,
}

/// Standard I/O configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StdioConfig {
    /// Inherit from parent
    Inherit,
    /// Pipe to/from process
    Piped,
    /// Redirect to file
    File(PathBuf),
    /// Discard output
    Null,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check command
    pub command: String,
    /// Health check arguments
    pub args: Vec<String>,
    /// Check interval
    pub interval: Duration,
    /// Timeout for each check
    pub timeout: Duration,
    /// Number of consecutive failures before considering unhealthy
    pub failure_threshold: u32,
}

/// State of a managed process
#[derive(Debug, Clone)]
pub enum ProcessState {
    /// Process is starting
    Starting,
    /// Process is running
    Running,
    /// Process is healthy
    Healthy,
    /// Process is unhealthy
    Unhealthy,
    /// Process is stopping
    Stopping,
    /// Process has stopped normally
    Stopped,
    /// Process crashed
    Crashed { exit_code: Option<i32> },
    /// Process was killed
    Killed,
    /// Process timed out
    TimedOut,
}

/// Resource usage tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Current memory usage (bytes)
    pub memory_bytes: u64,
    /// Peak memory usage (bytes)
    pub peak_memory_bytes: u64,
    /// Current CPU usage (percentage)
    pub cpu_percent: f64,
    /// Peak CPU usage (percentage)
    pub peak_cpu_percent: f64,
    /// Total CPU time (seconds)
    pub cpu_time_seconds: f64,
    /// Current disk usage (bytes)
    pub disk_bytes: u64,
    /// Peak disk usage (bytes)
    pub peak_disk_bytes: u64,
    /// Number of file descriptors
    pub file_descriptors: u32,
    /// Network bytes sent
    pub network_bytes_sent: u64,
    /// Network bytes received
    pub network_bytes_received: u64,
    /// Last updated timestamp
    pub last_updated: Instant,
}

/// Resource monitoring service
pub struct ResourceMonitor {
    /// Monitoring interval
    interval: Duration,
    /// Monitored processes
    processes: Arc<RwLock<HashMap<ProcessId, Arc<Mutex<ResourceUsage>>>>>,
    /// Monitor state
    state: Arc<Mutex<MonitorState>>,
}

/// State of the resource monitor
#[derive(Debug, Clone)]
pub enum MonitorState {
    /// Monitor is stopped
    Stopped,
    /// Monitor is running
    Running,
    /// Monitor is paused
    Paused,
    /// Monitor encountered an error
    Error(String),
}

/// Log collection service
pub struct LogCollector {
    /// Log storage directory
    log_dir: PathBuf,
    /// Active log streams
    streams: Arc<RwLock<HashMap<ProcessId, LogStream>>>,
    /// Collector state
    state: Arc<Mutex<CollectorState>>,
}

/// Log stream for a process
pub struct LogStream {
    /// Process identifier
    pub process_id: ProcessId,
    /// Stdout log file
    pub stdout_file: Option<File>,
    /// Stderr log file
    pub stderr_file: Option<File>,
    /// Log buffer
    pub buffer: Vec<LogEntry>,
    /// Buffer size limit
    pub buffer_limit: usize,
}

/// Individual log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Process identifier
    pub process_id: ProcessId,
    /// Log level
    pub level: LogLevel,
    /// Log message
    pub message: String,
    /// Stream type (stdout/stderr)
    pub stream: LogStream,
}

/// Log level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    /// Debug level
    Debug,
    /// Info level
    Info,
    /// Warning level
    Warn,
    /// Error level
    Error,
    /// Fatal level
    Fatal,
}

/// Log stream type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogStreamType {
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
}

/// State of the log collector
#[derive(Debug, Clone)]
pub enum CollectorState {
    /// Collector is stopped
    Stopped,
    /// Collector is running
    Running,
    /// Collector is paused
    Paused,
    /// Collector encountered an error
    Error(String),
}

/// State of the process manager
#[derive(Debug, Clone)]
pub enum ManagerState {
    /// Manager is stopped
    Stopped,
    /// Manager is running
    Running,
    /// Manager is paused
    Paused,
    /// Manager encountered an error
    Error(String),
}

impl ProcessManager {
    /// Create a new process manager
    pub fn new() -> Self {
        let log_dir = std::env::temp_dir().join("kaelix-test-logs");
        
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            resource_monitor: Arc::new(ResourceMonitor::new(Duration::from_secs(1))),
            log_collector: Arc::new(LogCollector::new(log_dir)),
            state: Arc::new(Mutex::new(ManagerState::Stopped)),
        }
    }

    /// Start the process manager
    pub async fn start(&self) -> Result<()> {
        info!("Starting process manager");
        
        {
            let mut state = self.state.lock().await;
            *state = ManagerState::Running;
        }

        // Start resource monitor
        self.resource_monitor.start().await?;
        
        // Start log collector
        self.log_collector.start().await?;
        
        info!("Process manager started");
        Ok(())
    }

    /// Stop the process manager
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping process manager");
        
        {
            let mut state = self.state.lock().await;
            *state = ManagerState::Stopped;
        }

        // Stop all processes
        let process_ids: Vec<ProcessId> = {
            let processes = self.processes.read().await;
            processes.keys().copied().collect()
        };

        for process_id in process_ids {
            if let Err(e) = self.stop_process(process_id).await {
                warn!("Failed to stop process {}: {}", process_id, e);
            }
        }

        // Stop resource monitor
        self.resource_monitor.stop().await?;
        
        // Stop log collector
        self.log_collector.stop().await?;
        
        info!("Process manager stopped");
        Ok(())
    }

    /// Spawn a new managed process
    pub async fn spawn_process(
        &self,
        config: ProcessConfig,
        constraints: ResourceConstraints,
    ) -> Result<ProcessId> {
        info!("Spawning process: {}", config.id);
        
        // Create command
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        
        if let Some(working_dir) = &config.working_dir {
            cmd.current_dir(working_dir);
        }
        
        for (key, value) in &config.env {
            cmd.env(key, value);
        }
        
        // Configure stdio
        cmd.stdin(self.stdio_config_to_stdio(&config.stdin)?);
        cmd.stdout(self.stdio_config_to_stdio(&config.stdout)?);
        cmd.stderr(self.stdio_config_to_stdio(&config.stderr)?);
        
        // Spawn process
        let handle = cmd.spawn()
            .with_context(|| format!("Failed to spawn process: {}", config.command))?;
        
        let managed_process = ManagedProcess {
            id: config.id,
            config: config.clone(),
            handle,
            constraints,
            state: Arc::new(Mutex::new(ProcessState::Starting)),
            started_at: Instant::now(),
            resource_usage: Arc::new(Mutex::new(ResourceUsage::default())),
        };
        
        // Register with resource monitor
        self.resource_monitor.register_process(config.id, managed_process.resource_usage.clone()).await;
        
        // Register with log collector
        self.log_collector.register_process(config.id).await?;
        
        // Store process
        {
            let mut processes = self.processes.write().await;
            processes.insert(config.id, managed_process);
        }
        
        // Update state to running
        {
            let processes = self.processes.read().await;
            if let Some(process) = processes.get(&config.id) {
                let mut state = process.state.lock().await;
                *state = ProcessState::Running;
            }
        }
        
        info!("Process spawned: {}", config.id);
        Ok(config.id)
    }

    /// Stop a managed process
    pub async fn stop_process(&self, process_id: ProcessId) -> Result<()> {
        info!("Stopping process: {}", process_id);
        
        let mut process_handle = {
            let mut processes = self.processes.write().await;
            processes.remove(&process_id)
        };
        
        if let Some(ref mut process) = process_handle {
            {
                let mut state = process.state.lock().await;
                *state = ProcessState::Stopping;
            }
            
            // Try graceful shutdown first
            if let Err(e) = process.handle.kill().await {
                warn!("Failed to kill process {}: {}", process_id, e);
            }
            
            {
                let mut state = process.state.lock().await;
                *state = ProcessState::Stopped;
            }
        }
        
        // Unregister from monitors
        self.resource_monitor.unregister_process(process_id).await;
        self.log_collector.unregister_process(process_id).await?;
        
        info!("Process stopped: {}", process_id);
        Ok(())
    }

    /// Get process state
    pub async fn get_process_state(&self, process_id: ProcessId) -> Option<ProcessState> {
        let processes = self.processes.read().await;
        if let Some(process) = processes.get(&process_id) {
            Some(process.state.lock().await.clone())
        } else {
            None
        }
    }

    /// Get resource usage for a process
    pub async fn get_resource_usage(&self, process_id: ProcessId) -> Option<ResourceUsage> {
        let processes = self.processes.read().await;
        if let Some(process) = processes.get(&process_id) {
            Some(process.resource_usage.lock().await.clone())
        } else {
            None
        }
    }

    /// Get all managed processes
    pub async fn get_all_processes(&self) -> HashMap<ProcessId, ProcessInfo> {
        let processes = self.processes.read().await;
        let mut result = HashMap::new();
        
        for (id, process) in processes.iter() {
            let state = process.state.lock().await.clone();
            let usage = process.resource_usage.lock().await.clone();
            
            result.insert(*id, ProcessInfo {
                id: *id,
                config: process.config.clone(),
                state,
                constraints: process.constraints.clone(),
                started_at: process.started_at,
                resource_usage: usage,
            });
        }
        
        result
    }

    /// Check if a process violates resource constraints
    pub async fn check_resource_violations(&self, process_id: ProcessId) -> Vec<ResourceViolation> {
        let mut violations = Vec::new();
        
        let processes = self.processes.read().await;
        if let Some(process) = processes.get(&process_id) {
            let usage = process.resource_usage.lock().await;
            let constraints = &process.constraints;
            
            if usage.memory_bytes > constraints.max_memory {
                violations.push(ResourceViolation::Memory {
                    current: usage.memory_bytes,
                    limit: constraints.max_memory,
                });
            }
            
            if usage.cpu_percent > constraints.max_cpu_cores * 100.0 {
                violations.push(ResourceViolation::Cpu {
                    current: usage.cpu_percent,
                    limit: constraints.max_cpu_cores * 100.0,
                });
            }
            
            if usage.disk_bytes > constraints.max_disk {
                violations.push(ResourceViolation::Disk {
                    current: usage.disk_bytes,
                    limit: constraints.max_disk,
                });
            }
            
            if usage.file_descriptors > constraints.max_file_descriptors {
                violations.push(ResourceViolation::FileDescriptors {
                    current: usage.file_descriptors,
                    limit: constraints.max_file_descriptors,
                });
            }
        }
        
        violations
    }

    // Private helper methods
    
    fn stdio_config_to_stdio(&self, config: &StdioConfig) -> Result<Stdio> {
        match config {
            StdioConfig::Inherit => Ok(Stdio::inherit()),
            StdioConfig::Piped => Ok(Stdio::piped()),
            StdioConfig::File(_) => Ok(Stdio::piped()), // Handle file redirection separately
            StdioConfig::Null => Ok(Stdio::null()),
        }
    }
}

impl ResourceMonitor {
    /// Create a new resource monitor
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            processes: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(Mutex::new(MonitorState::Stopped)),
        }
    }

    /// Start the resource monitor
    pub async fn start(&self) -> Result<()> {
        info!("Starting resource monitor");
        
        {
            let mut state = self.state.lock().await;
            *state = MonitorState::Running;
        }

        // Start monitoring loop
        let processes = self.processes.clone();
        let state = self.state.clone();
        let interval_duration = self.interval;
        
        tokio::spawn(async move {
            let mut interval = interval(interval_duration);
            
            loop {
                interval.tick().await;
                
                let current_state = state.lock().await.clone();
                if !matches!(current_state, MonitorState::Running) {
                    break;
                }
                
                // Update resource usage for all processes
                let process_map = processes.read().await;
                for (process_id, usage_arc) in process_map.iter() {
                    if let Err(e) = Self::update_resource_usage(*process_id, usage_arc.clone()).await {
                        debug!("Failed to update resource usage for process {}: {}", process_id, e);
                    }
                }
            }
        });
        
        info!("Resource monitor started");
        Ok(())
    }

    /// Stop the resource monitor
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping resource monitor");
        
        {
            let mut state = self.state.lock().await;
            *state = MonitorState::Stopped;
        }
        
        info!("Resource monitor stopped");
        Ok(())
    }

    /// Register a process for monitoring
    pub async fn register_process(&self, process_id: ProcessId, usage: Arc<Mutex<ResourceUsage>>) {
        let mut processes = self.processes.write().await;
        processes.insert(process_id, usage);
    }

    /// Unregister a process from monitoring
    pub async fn unregister_process(&self, process_id: ProcessId) {
        let mut processes = self.processes.write().await;
        processes.remove(&process_id);
    }

    // Private helper methods
    
    async fn update_resource_usage(
        _process_id: ProcessId,
        usage_arc: Arc<Mutex<ResourceUsage>>,
    ) -> Result<()> {
        // In a real implementation, this would read actual process statistics
        // For now, we'll simulate some basic usage
        let mut usage = usage_arc.lock().await;
        usage.last_updated = Instant::now();
        
        // Simulate some resource usage (in reality, this would read from /proc or similar)
        usage.memory_bytes = 1024 * 1024; // 1MB
        usage.cpu_percent = 5.0; // 5%
        usage.disk_bytes = 1024 * 1024; // 1MB
        usage.file_descriptors = 10;
        
        Ok(())
    }
}

impl LogCollector {
    /// Create a new log collector
    pub fn new(log_dir: PathBuf) -> Self {
        Self {
            log_dir,
            streams: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(Mutex::new(CollectorState::Stopped)),
        }
    }

    /// Start the log collector
    pub async fn start(&self) -> Result<()> {
        info!("Starting log collector");
        
        // Create log directory
        tokio::fs::create_dir_all(&self.log_dir).await
            .with_context(|| format!("Failed to create log directory: {:?}", self.log_dir))?;
        
        {
            let mut state = self.state.lock().await;
            *state = CollectorState::Running;
        }
        
        info!("Log collector started");
        Ok(())
    }

    /// Stop the log collector
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping log collector");
        
        {
            let mut state = self.state.lock().await;
            *state = CollectorState::Stopped;
        }
        
        // Flush all streams
        let mut streams = self.streams.write().await;
        for (_, stream) in streams.iter_mut() {
            if let Some(ref mut stdout_file) = stream.stdout_file {
                let _ = stdout_file.flush().await;
            }
            if let Some(ref mut stderr_file) = stream.stderr_file {
                let _ = stderr_file.flush().await;
            }
        }
        
        info!("Log collector stopped");
        Ok(())
    }

    /// Register a process for log collection
    pub async fn register_process(&self, process_id: ProcessId) -> Result<()> {
        let stdout_path = self.log_dir.join(format!("{}.stdout.log", process_id));
        let stderr_path = self.log_dir.join(format!("{}.stderr.log", process_id));
        
        let stdout_file = File::create(stdout_path).await?;
        let stderr_file = File::create(stderr_path).await?;
        
        let stream = LogStream {
            process_id,
            stdout_file: Some(stdout_file),
            stderr_file: Some(stderr_file),
            buffer: Vec::new(),
            buffer_limit: 10000, // Keep last 10k log entries in memory
        };
        
        let mut streams = self.streams.write().await;
        streams.insert(process_id, stream);
        
        Ok(())
    }

    /// Unregister a process from log collection
    pub async fn unregister_process(&self, process_id: ProcessId) -> Result<()> {
        let mut streams = self.streams.write().await;
        if let Some(mut stream) = streams.remove(&process_id) {
            // Flush remaining logs
            if let Some(ref mut stdout_file) = stream.stdout_file {
                let _ = stdout_file.flush().await;
            }
            if let Some(ref mut stderr_file) = stream.stderr_file {
                let _ = stderr_file.flush().await;
            }
        }
        
        Ok(())
    }

    /// Get recent logs for a process
    pub async fn get_recent_logs(&self, process_id: ProcessId, limit: usize) -> Vec<LogEntry> {
        let streams = self.streams.read().await;
        if let Some(stream) = streams.get(&process_id) {
            let mut logs = stream.buffer.clone();
            logs.truncate(limit);
            logs
        } else {
            Vec::new()
        }
    }
}

/// Process information
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process identifier
    pub id: ProcessId,
    /// Process configuration
    pub config: ProcessConfig,
    /// Current state
    pub state: ProcessState,
    /// Resource constraints
    pub constraints: ResourceConstraints,
    /// Start time
    pub started_at: Instant,
    /// Current resource usage
    pub resource_usage: ResourceUsage,
}

/// Resource constraint violations
#[derive(Debug, Clone)]
pub enum ResourceViolation {
    /// Memory usage violation
    Memory {
        /// Current usage
        current: u64,
        /// Limit
        limit: u64,
    },
    /// CPU usage violation
    Cpu {
        /// Current usage (percentage)
        current: f64,
        /// Limit (percentage)
        limit: f64,
    },
    /// Disk usage violation
    Disk {
        /// Current usage
        current: u64,
        /// Limit
        limit: u64,
    },
    /// File descriptor violation
    FileDescriptors {
        /// Current count
        current: u32,
        /// Limit
        limit: u32,
    },
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_process_manager_creation() {
        let manager = ProcessManager::new();
        assert!(matches!(
            *manager.state.lock().await,
            ManagerState::Stopped
        ));
    }

    #[tokio::test]
    async fn test_process_manager_lifecycle() {
        let manager = ProcessManager::new();
        
        // Start manager
        assert!(manager.start().await.is_ok());
        assert!(matches!(
            *manager.state.lock().await,
            ManagerState::Running
        ));

        // Stop manager
        assert!(manager.stop().await.is_ok());
        assert!(matches!(
            *manager.state.lock().await,
            ManagerState::Stopped
        ));
    }

    #[tokio::test]
    async fn test_process_spawning() {
        let manager = ProcessManager::new();
        let process_id = Uuid::new_v4();
        
        let config = ProcessConfig {
            id: process_id,
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            working_dir: None,
            env: HashMap::new(),
            stdin: StdioConfig::Null,
            stdout: StdioConfig::Piped,
            stderr: StdioConfig::Piped,
            timeout: Some(Duration::from_secs(5)),
            auto_restart: false,
            health_check: None,
        };
        
        let constraints = ResourceConstraints::default();
        
        manager.start().await.unwrap();
        let spawned_id = manager.spawn_process(config, constraints).await.unwrap();
        assert_eq!(spawned_id, process_id);
        
        // Process should be running
        let state = manager.get_process_state(process_id).await;
        assert!(matches!(state, Some(ProcessState::Running)));
        
        manager.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_resource_monitor() {
        let monitor = ResourceMonitor::new(Duration::from_millis(100));
        
        assert!(monitor.start().await.is_ok());
        assert!(matches!(
            *monitor.state.lock().await,
            MonitorState::Running
        ));

        assert!(monitor.stop().await.is_ok());
        assert!(matches!(
            *monitor.state.lock().await,
            MonitorState::Stopped
        ));
    }

    #[tokio::test]
    async fn test_log_collector() {
        let temp_dir = TempDir::new().unwrap();
        let collector = LogCollector::new(temp_dir.path().to_path_buf());
        
        assert!(collector.start().await.is_ok());
        assert!(matches!(
            *collector.state.lock().await,
            CollectorState::Running
        ));

        let process_id = Uuid::new_v4();
        assert!(collector.register_process(process_id).await.is_ok());
        
        let logs = collector.get_recent_logs(process_id, 10).await;
        assert_eq!(logs.len(), 0); // No logs yet
        
        assert!(collector.unregister_process(process_id).await.is_ok());
        assert!(collector.stop().await.is_ok());
    }
}