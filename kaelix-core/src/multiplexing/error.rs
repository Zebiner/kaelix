//! Stream multiplexing error types
//!
//! Comprehensive error handling for the ultra-high-performance stream multiplexing system.
//! Designed for zero-allocation error paths in performance-critical operations.

use thiserror::Error;
use std::fmt;

/// StreamId type for ultra-fast hash-based lookups
pub type StreamId = u64;

/// GroupId type for stream group management
pub type GroupId = u64;

/// WorkerId type for scheduler worker identification
pub type WorkerId = u32;

/// Priority type for stream scheduling
pub type Priority = u8;

/// Main error type for stream multiplexing operations
#[derive(Error, Debug, Clone)]
pub enum MultiplexerError {
    #[error("Registry error: {0}")]
    Registry(#[from] RegistryError),
    
    #[error("Scheduler error: {0}")]
    Scheduler(#[from] SchedulerError),
    
    #[error("Routing error: {0}")]
    Routing(#[from] RoutingError),
    
    #[error("Backpressure error: {0}")]
    Backpressure(#[from] BackpressureError),
    
    #[error("Topology error: {0}")]
    Topology(#[from] TopologyError),
    
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    
    #[error("Integration error: {0}")]
    Integration(#[from] IntegrationError),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("System error: {0}")]
    System(String),
}

/// Registry-specific errors
#[derive(Error, Debug, Clone)]
pub enum RegistryError {
    #[error("Stream already exists: {stream_id}")]
    StreamAlreadyExists { stream_id: StreamId },
    
    #[error("Stream not found: {stream_id}")]
    StreamNotFound { stream_id: StreamId },
    
    #[error("Registry capacity exceeded: {current}/{max}")]
    CapacityExceeded { current: usize, max: usize },
    
    #[error("Shard lock contention for stream: {stream_id}")]
    ShardContention { stream_id: StreamId },
    
    #[error("Memory allocation failed for stream: {stream_id}")]
    AllocationFailed { stream_id: StreamId },
    
    #[error("Invalid stream metadata")]
    InvalidMetadata,
}

/// Scheduler-specific errors
#[derive(Error, Debug, Clone)]
pub enum SchedulerError {
    #[error("Invalid priority level: {priority}")]
    InvalidPriority { priority: Priority },
    
    #[error("Queue overflow for worker: {worker_id}")]
    QueueOverflow { worker_id: WorkerId },
    
    #[error("Worker not available: {worker_id}")]
    WorkerUnavailable { worker_id: WorkerId },
    
    #[error("Task deadline exceeded: stream={stream_id}, deadline={deadline_ns}ns")]
    DeadlineExceeded { stream_id: StreamId, deadline_ns: u64 },
    
    #[error("Scheduling policy violation")]
    PolicyViolation,
    
    #[error("Fair scheduling constraint failed")]
    FairnessViolation,
}

/// Routing-specific errors
#[derive(Error, Debug, Clone)]
pub enum RoutingError {
    #[error("No routes found for topic: {topic}")]
    NoRoutesFound { topic: String },
    
    #[error("Invalid routing rule: {rule}")]
    InvalidRule { rule: String },
    
    #[error("Pattern compilation failed: {pattern}")]
    PatternCompilationFailed { pattern: String },
    
    #[error("Load balancing failed: no healthy targets")]
    LoadBalancingFailed,
    
    #[error("Routing table full: {current}/{max} entries")]
    RoutingTableFull { current: usize, max: usize },
    
    #[error("Cache miss for critical path")]
    CacheMiss,
}

/// Backpressure-specific errors
#[derive(Error, Debug, Clone)]
pub enum BackpressureError {
    #[error("Insufficient credits: stream={stream_id}, requested={requested}, available={available}")]
    InsufficientCredits { 
        stream_id: StreamId, 
        requested: i32, 
        available: i32 
    },
    
    #[error("Flow control violation: stream={stream_id}")]
    FlowControlViolation { stream_id: StreamId },
    
    #[error("Pressure propagation failed: stream={stream_id}")]
    PropagationFailed { stream_id: StreamId },
    
    #[error("Credit overflow: stream={stream_id}")]
    CreditOverflow { stream_id: StreamId },
    
    #[error("Global pressure critical: level={level}")]
    GlobalPressureCritical { level: u8 },
}

/// Topology-specific errors
#[derive(Error, Debug, Clone)]
pub enum TopologyError {
    #[error("Circular dependency detected: {cycle:?}")]
    CircularDependency { cycle: Vec<StreamId> },
    
    #[error("Invalid topology: {reason}")]
    InvalidTopology { reason: String },
    
    #[error("Dependency not found: dependent={dependent}, dependency={dependency}")]
    DependencyNotFound { dependent: StreamId, dependency: StreamId },
    
    #[error("Group operation failed: group_id={group_id}")]
    GroupOperationFailed { group_id: GroupId },
    
    #[error("Topology constraint violation")]
    ConstraintViolation,
}

/// Storage-specific errors
#[derive(Error, Debug, Clone)]
pub enum StorageError {
    #[error("Tier capacity exceeded: tier={tier}, current={current}, max={max}")]
    TierCapacityExceeded { tier: String, current: usize, max: usize },
    
    #[error("Promotion failed: stream={stream_id}, from={from_tier}, to={to_tier}")]
    PromotionFailed { stream_id: StreamId, from_tier: String, to_tier: String },
    
    #[error("Eviction failed: stream={stream_id}")]
    EvictionFailed { stream_id: StreamId },
    
    #[error("Memory limit exceeded: current={current_mb}MB, limit={limit_mb}MB")]
    MemoryLimitExceeded { current_mb: usize, limit_mb: usize },
    
    #[error("Compression failed: stream={stream_id}")]
    CompressionFailed { stream_id: StreamId },
    
    #[error("Persistence error: {reason}")]
    PersistenceError { reason: String },
}

/// Integration-specific errors
#[derive(Error, Debug, Clone)]
pub enum IntegrationError {
    #[error("Protocol integration failed: {protocol}")]
    ProtocolIntegration { protocol: String },
    
    #[error("Plugin integration failed: plugin={plugin_id}")]
    PluginIntegration { plugin_id: String },
    
    #[error("Telemetry integration failed")]
    TelemetryIntegration,
    
    #[error("Network integration failed: {reason}")]
    NetworkIntegration { reason: String },
    
    #[error("Configuration synchronization failed")]
    ConfigSynchronization,
}

/// Task creation errors
#[derive(Error, Debug, Clone)]
pub enum CreateStreamError {
    #[error("Invalid stream configuration: {reason}")]
    InvalidConfig { reason: String },
    
    #[error("Resource allocation failed")]
    ResourceAllocation,
    
    #[error("Stream limit exceeded: current={current}, max={max}")]
    LimitExceeded { current: usize, max: usize },
}

/// Stream retrieval errors
#[derive(Error, Debug, Clone)]
pub enum GetStreamError {
    #[error("Stream not found: {stream_id}")]
    NotFound { stream_id: StreamId },
    
    #[error("Stream not ready: {stream_id}")]
    NotReady { stream_id: StreamId },
    
    #[error("Access denied: {stream_id}")]
    AccessDenied { stream_id: StreamId },
}

/// Message sending errors
#[derive(Error, Debug, Clone)]
pub enum SendError {
    #[error("Stream not available: {stream_id}")]
    StreamUnavailable { stream_id: StreamId },
    
    #[error("Queue full: {stream_id}")]
    QueueFull { stream_id: StreamId },
    
    #[error("Message too large: size={size}, limit={limit}")]
    MessageTooLarge { size: usize, limit: usize },
    
    #[error("Routing failed: {stream_id}")]
    RoutingFailed { stream_id: StreamId },
}

/// Message receiving errors
#[derive(Error, Debug, Clone)]
pub enum ReceiveError {
    #[error("Stream closed: {stream_id}")]
    StreamClosed { stream_id: StreamId },
    
    #[error("Timeout: {stream_id}")]
    Timeout { stream_id: StreamId },
    
    #[error("Deserialization failed: {stream_id}")]
    DeserializationFailed { stream_id: StreamId },
}

/// Batch operation errors
#[derive(Error, Debug, Clone)]
pub enum BatchSendError {
    #[error("Partial batch failure: succeeded={succeeded}, failed={failed}")]
    PartialFailure { succeeded: usize, failed: usize },
    
    #[error("Batch too large: size={size}, limit={limit}")]
    BatchTooLarge { size: usize, limit: usize },
    
    #[error("All operations failed")]
    AllFailed,
}

/// Group creation errors
#[derive(Error, Debug, Clone)]
pub enum CreateGroupError {
    #[error("Invalid group configuration: {reason}")]
    InvalidConfig { reason: String },
    
    #[error("Group already exists: {group_id}")]
    AlreadyExists { group_id: GroupId },
    
    #[error("Resource allocation failed")]
    ResourceAllocation,
}

/// Lifecycle management errors
#[derive(Error, Debug, Clone)]
pub enum StartError {
    #[error("Already started")]
    AlreadyStarted,
    
    #[error("Resource initialization failed: {resource}")]
    ResourceInitialization { resource: String },
    
    #[error("Worker spawn failed")]
    WorkerSpawn,
}

#[derive(Error, Debug, Clone)]
pub enum StopError {
    #[error("Not running")]
    NotRunning,
    
    #[error("Graceful shutdown timeout")]
    ShutdownTimeout,
    
    #[error("Resource cleanup failed: {resource}")]
    ResourceCleanup { resource: String },
}

#[derive(Error, Debug, Clone)]
pub enum ShutdownError {
    #[error("Force shutdown required")]
    ForceRequired,
    
    #[error("Resource leak detected: {resources:?}")]
    ResourceLeak { resources: Vec<String> },
}

/// Flow control specific errors
#[derive(Error, Debug, Clone)]
pub enum FlowControlError {
    #[error("Credit allocation failed: stream={stream_id}")]
    AllocationFailed { stream_id: StreamId },
    
    #[error("Credit underflow: stream={stream_id}, attempted={attempted}")]
    CreditUnderflow { stream_id: StreamId, attempted: i32 },
    
    #[error("Rate limit exceeded: stream={stream_id}")]
    RateLimitExceeded { stream_id: StreamId },
}

/// Pressure propagation errors
#[derive(Error, Debug, Clone)]
pub enum PropagationError {
    #[error("Propagation cycle detected")]
    CycleDetected,
    
    #[error("Propagation depth exceeded: depth={depth}, max={max}")]
    DepthExceeded { depth: usize, max: usize },
    
    #[error("Notification delivery failed: stream={stream_id}")]
    NotificationFailed { stream_id: StreamId },
}

/// Memory management errors
#[derive(Error, Debug, Clone)]
pub enum MemoryError {
    #[error("Out of memory")]
    OutOfMemory,
    
    #[error("Memory fragmentation detected")]
    Fragmentation,
    
    #[error("NUMA allocation failed")]
    NumaAllocation,
}

/// Eviction policy errors
#[derive(Error, Debug, Clone)]
pub enum EvictionError {
    #[error("No candidates for eviction")]
    NoCandidates,
    
    #[error("Eviction policy failed: {policy}")]
    PolicyFailed { policy: String },
    
    #[error("Protected stream eviction attempted: {stream_id}")]
    ProtectedStream { stream_id: StreamId },
}

/// Batch processing errors
#[derive(Error, Debug, Clone)]
pub enum BatchError {
    #[error("Batch size invalid: {size}")]
    InvalidSize { size: usize },
    
    #[error("Batch processing timeout")]
    Timeout,
    
    #[error("Worker unavailable for batch")]
    WorkerUnavailable,
}

/// Optimization errors
#[derive(Error, Debug, Clone)]
pub enum OptimizationError {
    #[error("SIMD not supported")]
    SimdNotSupported,
    
    #[error("Optimization failed: {reason}")]
    OptimizationFailed { reason: String },
    
    #[error("Performance regression detected")]
    PerformanceRegression,
}

/// Convenience result types for each operation
pub type MultiplexerResult<T> = Result<T, MultiplexerError>;
pub type RegistryResult<T> = Result<T, RegistryError>;
pub type SchedulerResult<T> = Result<T, SchedulerError>;
pub type RoutingResult<T> = Result<T, RoutingError>;
pub type BackpressureResult<T> = Result<T, BackpressureError>;
pub type TopologyResult<T> = Result<T, TopologyError>;
pub type StorageResult<T> = Result<T, StorageError>;
pub type IntegrationResult<T> = Result<T, IntegrationError>;

/// Error context for debugging and telemetry
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub operation: String,
    pub stream_id: Option<StreamId>,
    pub timestamp_ns: u64,
    pub worker_id: Option<WorkerId>,
    pub additional_info: Vec<(String, String)>,
}

impl ErrorContext {
    pub fn new(operation: &str) -> Self {
        Self {
            operation: operation.to_string(),
            stream_id: None,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            worker_id: None,
            additional_info: Vec::new(),
        }
    }
    
    pub fn with_stream_id(mut self, stream_id: StreamId) -> Self {
        self.stream_id = Some(stream_id);
        self
    }
    
    pub fn with_worker_id(mut self, worker_id: WorkerId) -> Self {
        self.worker_id = Some(worker_id);
        self
    }
    
    pub fn with_info(mut self, key: &str, value: &str) -> Self {
        self.additional_info.push((key.to_string(), value.to_string()));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_context_creation() {
        let ctx = ErrorContext::new("test_operation")
            .with_stream_id(12345)
            .with_worker_id(1)
            .with_info("test_key", "test_value");
        
        assert_eq!(ctx.operation, "test_operation");
        assert_eq!(ctx.stream_id, Some(12345));
        assert_eq!(ctx.worker_id, Some(1));
        assert_eq!(ctx.additional_info.len(), 1);
        assert_eq!(ctx.additional_info[0], ("test_key".to_string(), "test_value".to_string()));
    }
    
    #[test]
    fn test_error_conversion() {
        let registry_error = RegistryError::StreamNotFound { stream_id: 123 };
        let multiplexer_error: MultiplexerError = registry_error.into();
        
        match multiplexer_error {
            MultiplexerError::Registry(RegistryError::StreamNotFound { stream_id }) => {
                assert_eq!(stream_id, 123);
            }
            _ => panic!("Unexpected error type"),
        }
    }
    
    #[test]
    fn test_error_display() {
        let error = RegistryError::StreamAlreadyExists { stream_id: 456 };
        assert!(error.to_string().contains("456"));
        
        let error = SchedulerError::InvalidPriority { priority: 255 };
        assert!(error.to_string().contains("255"));
    }
}