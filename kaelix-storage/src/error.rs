//! Storage error types and error handling utilities.
//!
//! This module provides comprehensive error handling for all storage operations,
//! with detailed error classifications and context information for debugging
//! and monitoring purposes.

use kaelix_cluster::types::NodeId;
use std::fmt;
use std::io;
use thiserror::Error;

/// Storage operation result type alias
pub type StorageResult<T> = Result<T, StorageError>;

/// Comprehensive error enumeration for storage operations
///
/// Provides detailed error classification for all storage-related failures,
/// enabling proper error handling, monitoring, and recovery strategies.
#[derive(Error, Debug)]
pub enum StorageError {
    /// Storage system initialization failed
    #[error("Storage initialization failed: {reason}")]
    InitializationFailed {
        /// Specific reason for initialization failure
        reason: String,
    },

    /// Storage system shutdown failed
    #[error("Storage shutdown failed: {reason}")]
    ShutdownFailed {
        /// Specific reason for shutdown failure
        reason: String,
    },

    /// Write operation failed
    #[error("Write operation failed at offset {offset}: {reason}")]
    WriteFailed {
        /// Offset where write failed
        offset: u64,
        /// Specific reason for write failure
        reason: String,
    },

    /// Read operation failed
    #[error("Read operation failed at offset {offset}: {reason}")]
    ReadFailed {
        /// Offset where read failed
        offset: u64,
        /// Specific reason for read failure
        reason: String,
    },

    /// Batch operation failed
    #[error("Batch operation failed: processed {processed} of {total} items: {reason}")]
    BatchOperationFailed {
        /// Number of successfully processed items
        processed: usize,
        /// Total number of items in batch
        total: usize,
        /// Specific reason for batch failure
        reason: String,
    },

    /// Invalid offset provided
    #[error("Invalid offset: {offset} (reason: {reason})")]
    InvalidOffset {
        /// The invalid offset value
        offset: u64,
        /// Reason why offset is invalid
        reason: String,
    },

    /// Message not found at specified offset
    #[error("Message not found at offset {offset}")]
    MessageNotFound {
        /// Offset where message was expected
        offset: u64,
    },

    /// Message corruption detected
    #[error("Message corruption detected at offset {offset}: {details}")]
    MessageCorrupted {
        /// Offset of corrupted message
        offset: u64,
        /// Details about the corruption
        details: String,
    },

    /// Storage backend unavailable
    #[error("Storage backend unavailable: {backend_type}")]
    BackendUnavailable {
        /// Type of storage backend that is unavailable
        backend_type: String,
    },

    /// Storage capacity exceeded
    #[error("Storage capacity exceeded: {used} / {capacity} bytes")]
    CapacityExceeded {
        /// Currently used storage in bytes
        used: u64,
        /// Total storage capacity in bytes
        capacity: u64,
    },

    /// Replication failure in distributed storage
    #[error("Replication failed to node {node_id}: {reason}")]
    ReplicationFailed {
        /// Node ID where replication failed
        node_id: NodeId,
        /// Specific reason for replication failure
        reason: String,
    },

    /// Consensus failure in distributed storage
    #[error("Consensus failed: required {required} replicas, got {achieved}")]
    ConsensusFailed {
        /// Number of replicas required for consensus
        required: usize,
        /// Number of replicas that achieved consensus
        achieved: usize,
    },

    /// Network partition detected
    #[error("Network partition detected: {details}")]
    NetworkPartition {
        /// Details about the network partition
        details: String,
    },

    /// Storage configuration error
    #[error("Storage configuration error: {parameter} = {value} ({reason})")]
    ConfigurationError {
        /// Configuration parameter name
        parameter: String,
        /// Invalid parameter value
        value: String,
        /// Reason why configuration is invalid
        reason: String,
    },

    /// Resource exhaustion error
    #[error("Resource exhausted: {resource} ({details})")]
    ResourceExhausted {
        /// Type of resource that was exhausted
        resource: String,
        /// Additional details about resource exhaustion
        details: String,
    },

    /// Timeout occurred during operation
    #[error("Operation timeout after {duration_ms}ms: {operation}")]
    OperationTimeout {
        /// Operation that timed out
        operation: String,
        /// Timeout duration in milliseconds
        duration_ms: u64,
    },

    /// Lock acquisition failed
    #[error("Lock acquisition failed: {lock_type} (waited {wait_duration_ms}ms)")]
    LockAcquisitionFailed {
        /// Type of lock that failed to acquire
        lock_type: String,
        /// Duration waited for lock in milliseconds
        wait_duration_ms: u64,
    },

    /// Serialization error
    #[error("Serialization error: {details}")]
    SerializationError {
        /// Details about serialization failure
        details: String,
    },

    /// Deserialization error
    #[error("Deserialization error at offset {offset}: {details}")]
    DeserializationError {
        /// Offset where deserialization failed
        offset: u64,
        /// Details about deserialization failure
        details: String,
    },

    /// File system operation failed
    #[error("File system operation failed: {operation} on {path}: {reason}")]
    FileSystemError {
        /// File system operation that failed
        operation: String,
        /// File path involved in the operation
        path: String,
        /// Specific reason for failure
        reason: String,
    },

    /// I/O operation error
    #[error("I/O error during {operation}: {source}")]
    IoError {
        /// Operation during which I/O error occurred
        operation: String,
        /// Underlying I/O error
        #[source]
        source: io::Error,
    },

    /// Protocol version mismatch
    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    ProtocolVersionMismatch {
        /// Expected protocol version
        expected: u32,
        /// Actual protocol version received
        actual: u32,
    },

    /// WAL (Write-Ahead Log) specific error
    #[error("WAL error: {operation} failed: {reason}")]
    WalError {
        /// WAL operation that failed
        operation: String,
        /// Specific reason for WAL failure
        reason: String,
    },

    /// Segment management error
    #[error("Segment error: {operation} on segment {segment_id}: {reason}")]
    SegmentError {
        /// Segment operation that failed
        operation: String,
        /// Segment identifier
        segment_id: u64,
        /// Specific reason for segment failure
        reason: String,
    },

    /// Recovery operation failed
    #[error("Recovery failed: {stage} (recovered {recovered} of {total} items)")]
    RecoveryFailed {
        /// Recovery stage where failure occurred
        stage: String,
        /// Number of successfully recovered items
        recovered: usize,
        /// Total number of items to recover
        total: usize,
    },

    /// Generic internal error for unexpected conditions
    #[error("Internal storage error: {message}")]
    InternalError {
        /// Error message describing the internal error
        message: String,
    },
}

impl StorageError {
    /// Check if error is transient and operation can be retried
    ///
    /// # Returns
    ///
    /// `true` if the error condition is likely temporary and the operation
    /// may succeed if retried after a short delay.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_storage::error::StorageError;
    ///
    /// let error = StorageError::OperationTimeout {
    ///     operation: "write".to_string(),
    ///     duration_ms: 5000,
    /// };
    ///
    /// if error.is_transient() {
    ///     println!("Operation can be retried");
    /// }
    /// ```
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::BackendUnavailable { .. }
                | Self::NetworkPartition { .. }
                | Self::OperationTimeout { .. }
                | Self::LockAcquisitionFailed { .. }
                | Self::ReplicationFailed { .. }
                | Self::ConsensusFailed { .. }
                | Self::ResourceExhausted { .. }
                | Self::IoError { .. }
        )
    }

    /// Check if error indicates data corruption
    ///
    /// # Returns
    ///
    /// `true` if the error indicates data corruption that may require
    /// recovery or repair operations.
    #[must_use]
    pub fn is_corruption(&self) -> bool {
        matches!(
            self,
            Self::MessageCorrupted { .. }
                | Self::DeserializationError { .. }
                | Self::ProtocolVersionMismatch { .. }
        )
    }

    /// Check if error is related to capacity constraints
    ///
    /// # Returns
    ///
    /// `true` if the error is due to capacity limits being reached.
    #[must_use]
    pub fn is_capacity_related(&self) -> bool {
        matches!(self, Self::CapacityExceeded { .. } | Self::ResourceExhausted { .. })
    }

    /// Check if error is configuration-related
    ///
    /// # Returns
    ///
    /// `true` if the error is due to invalid or incompatible configuration.
    #[must_use]
    pub fn is_configuration_error(&self) -> bool {
        matches!(self, Self::ConfigurationError { .. } | Self::ProtocolVersionMismatch { .. })
    }

    /// Get error severity level for monitoring and alerting
    ///
    /// # Returns
    ///
    /// [`ErrorSeverity`] indicating the severity level of this error.
    #[must_use]
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            // Critical errors requiring immediate attention
            Self::MessageCorrupted { .. }
            | Self::RecoveryFailed { .. }
            | Self::NetworkPartition { .. } => ErrorSeverity::Critical,

            // High severity errors affecting system operation
            Self::InitializationFailed { .. }
            | Self::ShutdownFailed { .. }
            | Self::BackendUnavailable { .. }
            | Self::CapacityExceeded { .. }
            | Self::ConsensusFailed { .. } => ErrorSeverity::High,

            // Medium severity errors affecting individual operations
            Self::WriteFailed { .. }
            | Self::ReadFailed { .. }
            | Self::BatchOperationFailed { .. }
            | Self::ReplicationFailed { .. }
            | Self::WalError { .. }
            | Self::SegmentError { .. } => ErrorSeverity::Medium,

            // Low severity errors for individual requests
            Self::InvalidOffset { .. }
            | Self::MessageNotFound { .. }
            | Self::OperationTimeout { .. }
            | Self::LockAcquisitionFailed { .. }
            | Self::SerializationError { .. }
            | Self::DeserializationError { .. } => ErrorSeverity::Low,

            // Configuration and setup issues
            Self::ConfigurationError { .. } | Self::ProtocolVersionMismatch { .. } => {
                ErrorSeverity::Medium
            }

            // System resource issues
            Self::ResourceExhausted { .. } | Self::FileSystemError { .. } | Self::IoError { .. } => {
                ErrorSeverity::High
            }

            // Catch-all for internal errors
            Self::InternalError { .. } => ErrorSeverity::High,
        }
    }

    /// Create a new I/O error with context
    ///
    /// # Parameters
    ///
    /// * `operation` - Description of the operation that failed
    /// * `source` - The underlying I/O error
    ///
    /// # Returns
    ///
    /// A new [`StorageError::IoError`] with the provided context.
    #[must_use]
    pub fn io_error(operation: impl Into<String>, source: io::Error) -> Self {
        Self::IoError { operation: operation.into(), source }
    }

    /// Create a new serialization error
    ///
    /// # Parameters
    ///
    /// * `details` - Details about the serialization failure
    ///
    /// # Returns
    ///
    /// A new [`StorageError::SerializationError`] with the provided details.
    #[must_use]
    pub fn serialization_error(details: impl Into<String>) -> Self {
        Self::SerializationError { details: details.into() }
    }

    /// Create a new deserialization error
    ///
    /// # Parameters
    ///
    /// * `offset` - Offset where deserialization failed
    /// * `details` - Details about the deserialization failure
    ///
    /// # Returns
    ///
    /// A new [`StorageError::DeserializationError`] with the provided context.
    #[must_use]
    pub fn deserialization_error(offset: u64, details: impl Into<String>) -> Self {
        Self::DeserializationError { offset, details: details.into() }
    }
}

/// Error severity levels for monitoring and alerting
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low severity - individual request failures
    Low,
    /// Medium severity - operational issues affecting multiple requests
    Medium,
    /// High severity - system-wide issues affecting availability
    High,
    /// Critical severity - data integrity or system stability issues
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        };
        write!(f, "{level}")
    }
}

/// Convert from bincode errors
impl From<bincode::Error> for StorageError {
    fn from(err: bincode::Error) -> Self {
        match *err {
            bincode::ErrorKind::Io(io_err) => Self::io_error("serialization", io_err),
            _ => Self::serialization_error(format!("bincode error: {err}")),
        }
    }
}

/// Convert from standard I/O errors
impl From<io::Error> for StorageError {
    fn from(err: io::Error) -> Self {
        Self::io_error("file operation", err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification() {
        let timeout_error = StorageError::OperationTimeout {
            operation: "write".to_string(),
            duration_ms: 5000,
        };

        assert!(timeout_error.is_transient());
        assert!(!timeout_error.is_corruption());
        assert!(!timeout_error.is_capacity_related());
        assert_eq!(timeout_error.severity(), ErrorSeverity::Low);

        let corruption_error = StorageError::MessageCorrupted {
            offset: 1234,
            details: "checksum mismatch".to_string(),
        };

        assert!(!corruption_error.is_transient());
        assert!(corruption_error.is_corruption());
        assert!(!corruption_error.is_capacity_related());
        assert_eq!(corruption_error.severity(), ErrorSeverity::Critical);

        let capacity_error = StorageError::CapacityExceeded { used: 1000, capacity: 800 };

        assert!(!capacity_error.is_transient());
        assert!(!capacity_error.is_corruption());
        assert!(capacity_error.is_capacity_related());
        assert_eq!(capacity_error.severity(), ErrorSeverity::High);
    }

    #[test]
    fn test_error_creation_helpers() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let storage_err = StorageError::io_error("read", io_err);

        if let StorageError::IoError { operation, .. } = storage_err {
            assert_eq!(operation, "read");
        } else {
            panic!("Expected IoError variant");
        }

        let ser_err = StorageError::serialization_error("invalid format");
        if let StorageError::SerializationError { details } = ser_err {
            assert_eq!(details, "invalid format");
        } else {
            panic!("Expected SerializationError variant");
        }

        let deser_err = StorageError::deserialization_error(42, "truncated data");
        if let StorageError::DeserializationError { offset, details } = deser_err {
            assert_eq!(offset, 42);
            assert_eq!(details, "truncated data");
        } else {
            panic!("Expected DeserializationError variant");
        }
    }

    #[test]
    fn test_severity_ordering() {
        assert!(ErrorSeverity::Low < ErrorSeverity::Medium);
        assert!(ErrorSeverity::Medium < ErrorSeverity::High);
        assert!(ErrorSeverity::High < ErrorSeverity::Critical);
    }

    #[test]
    fn test_error_display() {
        let error = StorageError::WriteFailed {
            offset: 1234,
            reason: "disk full".to_string(),
        };

        let error_string = format!("{error}");
        assert!(error_string.contains("Write operation failed"));
        assert!(error_string.contains("1234"));
        assert!(error_string.contains("disk full"));
    }
}