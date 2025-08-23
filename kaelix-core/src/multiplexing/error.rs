//! Stream multiplexing error types
//!
//! Comprehensive error handling for the ultra-high-performance stream multiplexing system.
//! Designed for zero-allocation error paths in performance-critical operations.

use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// StreamId type for ultra-fast hash-based lookups
pub type StreamId = u64;

/// GroupId type for stream group management
pub type GroupId = u64;

/// WorkerId type for scheduler worker identification
pub type WorkerId = u64;

/// Priority type for message prioritization (0 = highest, 255 = lowest)
pub type Priority = u8;

/// Result type for multiplexer operations
pub type MultiplexerResult<T> = Result<T, MultiplexerError>;

/// StreamId generator for unique stream identifiers
static STREAM_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// StreamId implementation
impl StreamId {
    /// Generate a new unique StreamId
    pub fn new() -> Self {
        STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    }
}

/// Comprehensive error types for stream multiplexing operations
///
/// Designed for zero-allocation error paths where possible.
/// Error variants are ordered by expected frequency for optimal branch prediction.
#[derive(Error, Debug, Clone)]
pub enum MultiplexerError {
    // High-frequency errors
    #[error("Stream operation error: {0}")]
    Stream(String),

    #[error("Message routing error: {0}")]
    Routing(String),

    #[error("Backpressure triggered: {0}")]
    Backpressure(String),

    // Medium-frequency errors
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Network error: {0}")]
    Network(String),

    // Low-frequency errors
    #[error("System error: {0}")]
    System(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Stream creation errors
#[derive(Error, Debug, Clone)]
pub enum CreateStreamError {
    #[error("Maximum streams exceeded: {current}/{max}")]
    MaxStreamsExceeded { current: usize, max: usize },

    #[error("Invalid stream configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Stream ID already exists: {stream_id}")]
    StreamIdExists { stream_id: StreamId },

    #[error("System error: {0}")]
    SystemError(String),
}

/// Stream lookup errors
#[derive(Error, Debug, Clone)]
pub enum GetStreamError {
    #[error("Stream not found: {stream_id}")]
    NotFound { stream_id: StreamId },

    #[error("Stream access denied: {stream_id}")]
    AccessDenied { stream_id: StreamId },

    #[error("System error: {0}")]
    SystemError(String),
}

/// Message sending errors
#[derive(Error, Debug, Clone)]
pub enum SendError {
    #[error("Stream not found: {stream_id}")]
    StreamNotFound { stream_id: StreamId },

    #[error("Stream buffer full: {stream_id}")]
    BufferFull { stream_id: StreamId },

    #[error("Message too large: {size} bytes (max: {max_size})")]
    MessageTooLarge { size: usize, max_size: usize },

    #[error("Backpressure: {stream_id}")]
    Backpressure { stream_id: StreamId },

    #[error("Stream closed: {stream_id}")]
    StreamClosed { stream_id: StreamId },

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Encoding error: {0}")]
    EncodingError(String),
}

/// Routing errors
#[derive(Error, Debug, Clone)]
pub enum RoutingError {
    #[error("No route found for topic: {topic}")]
    NoRoute { topic: String },

    #[error("Route table full")]
    RouteTableFull,

    #[error("Invalid routing pattern: {pattern}")]
    InvalidPattern { pattern: String },

    #[error("System error: {0}")]
    SystemError(String),
}

/// Backpressure control errors
#[derive(Error, Debug, Clone)]
pub enum BackpressureError {
    #[error("Backpressure threshold exceeded: {current}/{threshold}")]
    ThresholdExceeded { current: f64, threshold: f64 },

    #[error("Invalid backpressure configuration: {0}")]
    InvalidConfiguration(String),

    #[error("System error: {0}")]
    SystemError(String),
}

/// Scheduler errors
#[derive(Error, Debug, Clone)]
pub enum SchedulerError {
    #[error("Queue full: {queue_id}")]
    QueueFull { queue_id: String },

    #[error("Invalid priority: {priority}")]
    InvalidPriority { priority: u8 },

    #[error("Worker overloaded: {worker_id}")]
    WorkerOverloaded { worker_id: WorkerId },

    #[error("System error: {0}")]
    SystemError(String),
}

/// Registry errors
#[derive(Error, Debug, Clone)]
pub enum RegistryError {
    #[error("Stream not found: {stream_id}")]
    StreamNotFound { stream_id: StreamId },

    #[error("Stream already exists: {stream_id}")]
    StreamExists { stream_id: StreamId },

    #[error("Registry full: {capacity}")]
    RegistryFull { capacity: usize },

    #[error("Invalid stream metadata: {0}")]
    InvalidMetadata(String),

    #[error("System error: {0}")]
    SystemError(String),
}

/// Unified error conversion implementations
impl From<CreateStreamError> for MultiplexerError {
    fn from(err: CreateStreamError) -> Self {
        match err {
            CreateStreamError::MaxStreamsExceeded { .. } => {
                MultiplexerError::ResourceExhausted(err.to_string())
            },
            CreateStreamError::InvalidConfiguration(_) => {
                MultiplexerError::Configuration(err.to_string())
            },
            CreateStreamError::StreamIdExists { .. } => MultiplexerError::Stream(err.to_string()),
            CreateStreamError::SystemError(_) => MultiplexerError::System(err.to_string()),
        }
    }
}

impl From<SendError> for MultiplexerError {
    fn from(err: SendError) -> Self {
        match err {
            SendError::StreamNotFound { .. } | SendError::StreamClosed { .. } => {
                MultiplexerError::Stream(err.to_string())
            },
            SendError::BufferFull { .. } | SendError::MessageTooLarge { .. } => {
                MultiplexerError::ResourceExhausted(err.to_string())
            },
            SendError::Backpressure { .. } => MultiplexerError::Backpressure(err.to_string()),
            SendError::NetworkError(_) => MultiplexerError::Network(err.to_string()),
            SendError::EncodingError(_) => MultiplexerError::Internal(err.to_string()),
        }
    }
}

impl From<RoutingError> for MultiplexerError {
    fn from(err: RoutingError) -> Self {
        match err {
            RoutingError::NoRoute { .. } | RoutingError::InvalidPattern { .. } => {
                MultiplexerError::Routing(err.to_string())
            },
            RoutingError::RouteTableFull => MultiplexerError::ResourceExhausted(err.to_string()),
            RoutingError::SystemError(_) => MultiplexerError::System(err.to_string()),
        }
    }
}

impl From<BackpressureError> for MultiplexerError {
    fn from(err: BackpressureError) -> Self {
        match err {
            BackpressureError::ThresholdExceeded { .. } => {
                MultiplexerError::Backpressure(err.to_string())
            },
            BackpressureError::InvalidConfiguration(_) => {
                MultiplexerError::Configuration(err.to_string())
            },
            BackpressureError::SystemError(_) => MultiplexerError::System(err.to_string()),
        }
    }
}

impl From<SchedulerError> for MultiplexerError {
    fn from(err: SchedulerError) -> Self {
        match err {
            SchedulerError::QueueFull { .. } | SchedulerError::WorkerOverloaded { .. } => {
                MultiplexerError::ResourceExhausted(err.to_string())
            },
            SchedulerError::InvalidPriority { .. } => {
                MultiplexerError::Configuration(err.to_string())
            },
            SchedulerError::SystemError(_) => MultiplexerError::System(err.to_string()),
        }
    }
}

impl From<RegistryError> for MultiplexerError {
    fn from(err: RegistryError) -> Self {
        match err {
            RegistryError::StreamNotFound { .. } | RegistryError::StreamExists { .. } => {
                MultiplexerError::Stream(err.to_string())
            },
            RegistryError::RegistryFull { .. } => {
                MultiplexerError::ResourceExhausted(err.to_string())
            },
            RegistryError::InvalidMetadata(_) => MultiplexerError::Configuration(err.to_string()),
            RegistryError::SystemError(_) => MultiplexerError::System(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_id_generation() {
        let id1 = StreamId::new();
        let id2 = StreamId::new();
        let id3 = StreamId::new();

        assert!(id1 > 0);
        assert!(id2 > id1);
        assert!(id3 > id2);
    }

    #[test]
    fn test_error_conversion() {
        let create_err = CreateStreamError::MaxStreamsExceeded { current: 100, max: 100 };
        let multiplexer_err: MultiplexerError = create_err.into();

        match multiplexer_err {
            MultiplexerError::ResourceExhausted(_) => {},
            _ => panic!("Expected ResourceExhausted error"),
        }
    }

    #[test]
    fn test_send_error_conversion() {
        let send_err = SendError::BufferFull { stream_id: 123 };
        let multiplexer_err: MultiplexerError = send_err.into();

        match multiplexer_err {
            MultiplexerError::ResourceExhausted(_) => {},
            _ => panic!("Expected ResourceExhausted error"),
        }
    }

    #[test]
    fn test_backpressure_error() {
        let bp_err = BackpressureError::ThresholdExceeded { current: 0.95, threshold: 0.80 };
        let multiplexer_err: MultiplexerError = bp_err.into();

        match multiplexer_err {
            MultiplexerError::Backpressure(_) => {},
            _ => panic!("Expected Backpressure error"),
        }
    }
}
