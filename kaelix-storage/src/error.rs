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

    /// Seek operation failed
    #[error("Seek operation failed to offset {offset}: {reason}")]
    SeekFailed {
        /// Target offset for seek operation
        offset: u64,
        /// Specific reason for seek failure
        reason: String,
    },

    /// File system operation failed
    #[error("File system operation '{operation}' failed on path '{path}': {reason}")]
    FileSystemError {
        /// File system operation that failed
        operation: String,
        /// File path involved in the operation
        path: String,
        /// Specific reason for file system failure
        reason: String,
    },

    /// Storage backend unavailable
    #[error("Storage backend unavailable: {reason}")]
    BackendUnavailable {
        /// Specific reason for backend unavailability
        reason: String,
    },

    /// Data corruption detected
    #[error("Data corruption detected at offset {offset}: {details}")]
    DataCorrupted {
        /// Offset where corruption was detected
        offset: u64,
        /// Details about the corruption
        details: String,
    },

    /// Message corruption detected
    #[error("Message corruption detected at offset {offset}: {details}")]
    MessageCorrupted {
        /// Offset where message corruption was detected
        offset: u64,
        /// Details about the message corruption
        details: String,
    },

    /// Message not found
    #[error("Message with ID {message_id} not found")]
    MessageNotFound {
        /// Message ID that was not found
        message_id: String,
    },

    /// Sequence number not found
    #[error("Sequence number {sequence} not found")]
    SequenceNotFound {
        /// Sequence number that was not found
        sequence: u64,
    },

    /// Invalid offset specified
    #[error("Invalid offset: {offset} exceeds segment size {segment_size}")]
    InvalidOffset {
        /// Invalid offset value
        offset: u64,
        /// Current segment size
        segment_size: u64,
    },

    /// Serialization failed
    #[error("Serialization failed: {details}")]
    SerializationError {
        /// Details about serialization failure
        details: String,
    },

    /// Deserialization failed
    #[error("Deserialization failed at offset {offset}: {details}")]
    DeserializationError {
        /// Offset where deserialization failed
        offset: u64,
        /// Details about deserialization failure
        details: String,
    },

    /// Lock acquisition failed
    #[error("Lock acquisition failed for resource '{resource}': {reason}")]
    LockAcquisitionFailed {
        /// Resource that couldn't be locked
        resource: String,
        /// Specific reason for lock failure
        reason: String,
    },

    /// Operation timeout
    #[error("Operation timeout after {duration_ms}ms: {operation}")]
    OperationTimeout {
        /// Operation that timed out
        operation: String,
        /// Timeout duration in milliseconds
        duration_ms: u64,
    },

    /// Network partition detected
    #[error("Network partition detected: isolated from {node_count} nodes")]
    NetworkPartition {
        /// Number of nodes that are unreachable
        node_count: usize,
    },

    /// Replication failed
    #[error("Replication failed to {node_id}: {reason}")]
    ReplicationFailed {
        /// Node ID where replication failed
        node_id: NodeId,
        /// Specific reason for replication failure
        reason: String,
    },

    /// Consensus operation failed
    #[error("Consensus failed: {operation} (term: {term})")]
    ConsensusFailed {
        /// Consensus operation that failed
        operation: String,
        /// Raft term during which failure occurred
        term: u64,
    },

    /// Resource exhausted
    #[error("Resource exhausted: {resource} limit exceeded ({current}/{limit})")]
    ResourceExhausted {
        /// Resource that was exhausted
        resource: String,
        /// Current resource usage
        current: u64,
        /// Resource limit
        limit: u64,
    },

    /// Configuration error
    #[error("Configuration error: {parameter} is invalid: {reason}")]
    ConfigurationError {
        /// Configuration parameter with invalid value
        parameter: String,
        /// Reason why the configuration is invalid
        reason: String,
    },

    /// Authentication failed
    #[error("Authentication failed for node {node_id}: {reason}")]
    AuthenticationFailed {
        /// Node ID that failed authentication
        node_id: NodeId,
        /// Reason for authentication failure
        reason: String,
    },

    /// Authorization failed
    #[error("Authorization failed: operation '{operation}' not permitted")]
    AuthorizationFailed {
        /// Operation that was not authorized
        operation: String,
    },

    /// Storage capacity exceeded
    #[error("Storage capacity exceeded: {used}/{capacity} bytes used")]
    CapacityExceeded {
        /// Currently used storage in bytes
        used: u64,
        /// Total storage capacity in bytes
        capacity: u64,
    },

    /// Version mismatch
    #[error(
        "Version mismatch: {component} version {actual} incompatible with expected {expected}"
    )]
    VersionMismatch {
        /// Component with version mismatch
        component: String,
        /// Actual version found
        actual: String,
        /// Expected version
        expected: String,
    },

    /// Operation not supported
    #[error("Operation not supported: {operation} - {reason}")]
    OperationNotSupported {
        /// Unsupported operation
        operation: String,
        /// Reason why operation is not supported
        reason: String,
    },

    /// Invalid operation state
    #[error("Invalid operation state: cannot perform '{operation}' in state '{state}'")]
    InvalidState {
        /// Operation being attempted
        operation: String,
        /// Current state that prevents the operation
        state: String,
    },

    /// I/O operation error
    #[error("I/O error during {operation}: {source}")]
    IoError {
        /// Operation during which I/O error occurred
        operation: String,
        /// Underlying I/O error (Note: io::Error doesn't implement Clone)
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

    /// Transaction operation failed
    #[error("Transaction {transaction_id} failed: {reason}")]
    TransactionFailed {
        /// Transaction identifier
        transaction_id: u64,
        /// Specific reason for transaction failure
        reason: String,
    },

    /// Replay operation failed
    #[error("Replay operation failed: {reason}")]
    ReplayError {
        /// Specific reason for replay failure
        reason: String,
    },

    /// Internal storage error (catch-all for unexpected errors)
    #[error("Internal storage error: {message}")]
    InternalError {
        /// Error message describing the internal error
        message: String,
    },
}

// Note: We cannot implement Clone for StorageError because io::Error doesn't implement Clone
// If cloning is needed, we need to convert the error to a cloneable form first

// Conversion from ConversionError to StorageError
impl From<crate::integration::ConversionError> for StorageError {
    fn from(err: crate::integration::ConversionError) -> Self {
        match err {
            crate::integration::ConversionError::SerializationFailed { reason } => {
                Self::SerializationError { details: reason }
            },
            crate::integration::ConversionError::DeserializationFailed { reason } => {
                Self::DeserializationError { offset: 0, details: reason }
            },
            crate::integration::ConversionError::InvalidFormat { details } => {
                Self::SerializationError { details }
            },
            crate::integration::ConversionError::TopicConversionFailed { topic } => {
                Self::SerializationError { details: format!("Topic conversion failed: {}", topic) }
            },
        }
    }
}

impl StorageError {
    /// Check if error is transient and operation can be retried
    ///
    /// # Returns
    ///
    /// `true` if the error condition might be temporary and retrying the operation
    /// could succeed, `false` if the error is permanent.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use kaelix_storage::StorageError;
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
        )
    }

    /// Get error severity level
    ///
    /// # Returns
    ///
    /// Error severity classification for logging and monitoring purposes.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use kaelix_storage::{StorageError, ErrorSeverity};
    ///
    /// let error = StorageError::DataCorrupted {
    ///     offset: 1024,
    ///     details: "checksum mismatch".to_string(),
    /// };
    ///
    /// match error.severity() {
    ///     ErrorSeverity::Critical => println!("Critical error requires immediate attention"),
    ///     ErrorSeverity::High => println!("High severity error"),
    ///     ErrorSeverity::Medium => println!("Medium severity error"),
    ///     ErrorSeverity::Low => println!("Low severity error"),
    /// }
    /// ```
    #[must_use]
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::DataCorrupted { .. }
            | Self::MessageCorrupted { .. }
            | Self::CapacityExceeded { .. }
            | Self::InternalError { .. } => ErrorSeverity::Critical,

            Self::InitializationFailed { .. }
            | Self::ShutdownFailed { .. }
            | Self::BackendUnavailable { .. }
            | Self::ConsensusFailed { .. }
            | Self::AuthenticationFailed { .. }
            | Self::AuthorizationFailed { .. } => ErrorSeverity::High,

            Self::WriteFailed { .. }
            | Self::ReadFailed { .. }
            | Self::ReplicationFailed { .. }
            | Self::RecoveryFailed { .. }
            | Self::TransactionFailed { .. }
            | Self::ReplayError { .. }
            | Self::OperationTimeout { .. }
            | Self::NetworkPartition { .. } => ErrorSeverity::Medium,

            Self::SeekFailed { .. }
            | Self::FileSystemError { .. }
            | Self::MessageNotFound { .. }
            | Self::SequenceNotFound { .. }
            | Self::InvalidOffset { .. }
            | Self::SerializationError { .. }
            | Self::DeserializationError { .. }
            | Self::LockAcquisitionFailed { .. }
            | Self::ResourceExhausted { .. }
            | Self::ConfigurationError { .. }
            | Self::VersionMismatch { .. }
            | Self::OperationNotSupported { .. }
            | Self::InvalidState { .. }
            | Self::IoError { .. }
            | Self::ProtocolVersionMismatch { .. }
            | Self::WalError { .. }
            | Self::SegmentError { .. } => ErrorSeverity::Low,
        }
    }

    /// Create a file system error
    ///
    /// # Parameters
    ///
    /// * `operation` - File system operation that failed
    /// * `path` - Path involved in the operation
    /// * `reason` - Reason for the failure
    ///
    /// # Returns
    ///
    /// A new [`StorageError::FileSystemError`] instance.
    #[must_use]
    pub fn file_system_error(operation: &str, path: &str, reason: &str) -> Self {
        Self::FileSystemError {
            operation: operation.to_string(),
            path: path.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create a data corruption error
    ///
    /// # Parameters
    ///
    /// * `offset` - Offset where corruption was detected
    /// * `details` - Details about the corruption
    ///
    /// # Returns
    ///
    /// A new [`StorageError::DataCorrupted`] instance.
    #[must_use]
    pub fn data_corrupted(offset: u64, details: &str) -> Self {
        Self::DataCorrupted { offset, details: details.to_string() }
    }

    /// Create an I/O error
    ///
    /// # Parameters
    ///
    /// * `operation` - I/O operation that failed
    /// * `source` - Underlying I/O error
    ///
    /// # Returns
    ///
    /// A new [`StorageError::IoError`] instance.
    #[must_use]
    pub fn io_error(operation: &str, source: io::Error) -> Self {
        Self::IoError { operation: operation.to_string(), source }
    }

    /// Create a serialization error
    ///
    /// # Parameters
    ///
    /// * `details` - Details about the serialization failure
    ///
    /// # Returns
    ///
    /// A new [`StorageError::SerializationError`] instance.
    #[must_use]
    pub fn serialization_error(details: &str) -> Self {
        Self::SerializationError { details: details.to_string() }
    }

    /// Create a deserialization error
    ///
    /// # Parameters
    ///
    /// * `offset` - Offset where deserialization failed
    /// * `details` - Details about the deserialization failure
    ///
    /// # Returns
    ///
    /// A new [`StorageError::DeserializationError`] instance.
    #[must_use]
    pub fn deserialization_error(offset: u64, details: &str) -> Self {
        Self::DeserializationError { offset, details: details.to_string() }
    }

    /// Create a transaction failed error
    ///
    /// # Parameters
    ///
    /// * `transaction_id` - Transaction identifier
    /// * `reason` - Reason for the transaction failure
    ///
    /// # Returns
    ///
    /// A new [`StorageError::TransactionFailed`] instance.
    #[must_use]
    pub fn transaction_failed(transaction_id: u64, reason: &str) -> Self {
        Self::TransactionFailed { transaction_id, reason: reason.to_string() }
    }

    /// Create a replay error
    ///
    /// # Parameters
    ///
    /// * `reason` - Reason for the replay failure
    ///
    /// # Returns
    ///
    /// A new [`StorageError::ReplayError`] instance.
    #[must_use]
    pub fn replay_error(reason: &str) -> Self {
        Self::ReplayError { reason: reason.to_string() }
    }

    /// Create a message not found error
    ///
    /// # Parameters
    ///
    /// * `message_id` - Message ID that was not found
    ///
    /// # Returns
    ///
    /// A new [`StorageError::MessageNotFound`] instance.
    #[must_use]
    pub fn message_not_found(message_id: &str) -> Self {
        Self::MessageNotFound { message_id: message_id.to_string() }
    }

    /// Convert StorageError to a cloneable version by converting io::Error to String
    ///
    /// This method converts the error to a form that can be cloned, which is useful
    /// when you need to share errors across threads or store them in structures
    /// that require Clone.
    #[must_use]
    pub fn to_cloneable(&self) -> CloneableStorageError {
        match self {
            Self::IoError { operation, source } => CloneableStorageError::IoError {
                operation: operation.clone(),
                error_message: source.to_string(),
            },
            _ => CloneableStorageError::Other(format!("{}", self)),
        }
    }
}

/// A cloneable version of StorageError
///
/// This type can be used when you need to clone storage errors,
/// particularly when dealing with io::Error which doesn't implement Clone.
#[derive(Debug, Clone)]
pub enum CloneableStorageError {
    /// I/O error with error message as string
    IoError {
        /// Operation during which I/O error occurred
        operation: String,
        /// String representation of the I/O error
        error_message: String,
    },
    /// Other errors as formatted strings
    Other(String),
}

impl fmt::Display for CloneableStorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError { operation, error_message } => {
                write!(f, "I/O error during {}: {}", operation, error_message)
            },
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for CloneableStorageError {}

/// Error severity levels for monitoring and alerting
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low severity - operational issues that don't affect functionality
    Low,
    /// Medium severity - degraded performance or partial failures
    Medium,
    /// High severity - significant operational impact
    High,
    /// Critical severity - system failure or data integrity issues
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::High => write!(f, "HIGH"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Error recovery strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Retry the operation
    Retry,
    /// Fallback to alternative approach
    Fallback,
    /// Abort the operation
    Abort,
    /// Escalate to higher level
    Escalate,
}

impl StorageError {
    /// Get recommended recovery strategy for this error
    ///
    /// # Returns
    ///
    /// Recommended recovery strategy based on error type and context.
    #[must_use]
    pub fn recovery_strategy(&self) -> RecoveryStrategy {
        match self {
            // Transient errors can be retried
            Self::OperationTimeout { .. }
            | Self::LockAcquisitionFailed { .. }
            | Self::ResourceExhausted { .. } => RecoveryStrategy::Retry,

            // Network and replication issues may have fallbacks
            Self::NetworkPartition { .. }
            | Self::ReplicationFailed { .. }
            | Self::BackendUnavailable { .. } => RecoveryStrategy::Fallback,

            // Data corruption and critical errors need escalation
            Self::DataCorrupted { .. }
            | Self::MessageCorrupted { .. }
            | Self::InternalError { .. } => RecoveryStrategy::Escalate,

            // Configuration and permanent errors should abort
            Self::ConfigurationError { .. }
            | Self::AuthorizationFailed { .. }
            | Self::VersionMismatch { .. }
            | Self::OperationNotSupported { .. } => RecoveryStrategy::Abort,

            // Most other errors can be retried initially
            _ => RecoveryStrategy::Retry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let fs_error = StorageError::file_system_error("write", "/tmp/test", "permission denied");
        if let StorageError::FileSystemError { operation, path, reason } = fs_error {
            assert_eq!(operation, "write");
            assert_eq!(path, "/tmp/test");
            assert_eq!(reason, "permission denied");
        } else {
            panic!("Expected FileSystemError variant");
        }

        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let storage_error = StorageError::io_error("read", io_error);
        if let StorageError::IoError { operation, .. } = storage_error {
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

        let tx_err = StorageError::transaction_failed(123, "timeout");
        if let StorageError::TransactionFailed { transaction_id, reason } = tx_err {
            assert_eq!(transaction_id, 123);
            assert_eq!(reason, "timeout");
        } else {
            panic!("Expected TransactionFailed variant");
        }

        let replay_err = StorageError::replay_error("session not found");
        if let StorageError::ReplayError { reason } = replay_err {
            assert_eq!(reason, "session not found");
        } else {
            panic!("Expected ReplayError variant");
        }

        let msg_err = StorageError::message_not_found("msg-123");
        if let StorageError::MessageNotFound { message_id } = msg_err {
            assert_eq!(message_id, "msg-123");
        } else {
            panic!("Expected MessageNotFound variant");
        }
    }

    #[test]
    fn test_cloneable_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let storage_error = StorageError::io_error("read", io_error);

        let cloneable = storage_error.to_cloneable();
        let cloned = cloneable.clone();

        if let CloneableStorageError::IoError { operation, error_message } = cloned {
            assert_eq!(operation, "read");
            assert!(error_message.contains("access denied"));
        } else {
            panic!("Expected IoError variant in cloneable");
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
        let error = StorageError::WriteFailed { offset: 1234, reason: "disk full".to_string() };

        let error_string = format!("{error}");
        assert!(error_string.contains("Write operation failed"));
        assert!(error_string.contains("1234"));
        assert!(error_string.contains("disk full"));

        let seq_error = StorageError::SequenceNotFound { sequence: 99 };
        let seq_string = format!("{seq_error}");
        assert!(seq_string.contains("Sequence number 99 not found"));

        let offset_error = StorageError::InvalidOffset { offset: 2000, segment_size: 1000 };
        let offset_string = format!("{offset_error}");
        assert!(offset_string.contains("Invalid offset: 2000 exceeds segment size 1000"));

        let msg_error = StorageError::MessageNotFound { message_id: "msg-456".to_string() };
        let msg_string = format!("{msg_error}");
        assert!(msg_string.contains("Message with ID msg-456 not found"));
    }

    #[test]
    fn test_recovery_strategies() {
        assert_eq!(
            StorageError::OperationTimeout { operation: "write".to_string(), duration_ms: 1000 }
                .recovery_strategy(),
            RecoveryStrategy::Retry
        );

        assert_eq!(
            StorageError::DataCorrupted { offset: 0, details: "checksum mismatch".to_string() }
                .recovery_strategy(),
            RecoveryStrategy::Escalate
        );

        assert_eq!(
            StorageError::ConfigurationError {
                parameter: "max_size".to_string(),
                reason: "negative value".to_string(),
            }
            .recovery_strategy(),
            RecoveryStrategy::Abort
        );
    }
}
