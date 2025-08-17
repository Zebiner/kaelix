//! Protocol-specific error types and handling.

use thiserror::Error;

/// Protocol-specific error types for frame handling.
#[derive(Error, Debug)]
pub enum ProtocolError {
    /// Invalid frame magic bytes.
    #[error("Invalid frame magic bytes: expected {expected:?}, got {actual:?}")]
    InvalidMagic {
        /// Expected magic bytes.
        expected: [u8; 4],
        /// Actual magic bytes found.
        actual: [u8; 4],
    },

    /// Unsupported protocol version.
    #[error("Unsupported protocol version: {version} (supported: {supported})")]
    UnsupportedVersion {
        /// Protocol version found.
        version: u8,
        /// Supported version.
        supported: u8,
    },

    /// Invalid frame type.
    #[error("Invalid frame type: {frame_type:#04x}")]
    InvalidFrameType {
        /// Invalid frame type value.
        frame_type: u16,
    },

    /// Frame size exceeds maximum allowed.
    #[error("Frame size {actual} exceeds maximum {max}")]
    FrameTooLarge {
        /// Actual frame size.
        actual: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Payload size exceeds maximum allowed.
    #[error("Payload size {actual} exceeds maximum {max}")]
    PayloadTooLarge {
        /// Actual payload size.
        actual: u32,
        /// Maximum allowed size.
        max: u32,
    },

    /// Frame buffer too small to contain header.
    #[error("Frame buffer too small: need at least {required} bytes, got {actual}")]
    BufferTooSmall {
        /// Required buffer size.
        required: usize,
        /// Actual buffer size.
        actual: usize,
    },

    /// Checksum mismatch indicating corruption.
    #[error("Checksum mismatch: expected {expected:#08x}, computed {actual:#08x}")]
    ChecksumMismatch {
        /// Expected checksum from frame.
        expected: u32,
        /// Computed checksum.
        actual: u32,
    },

    /// Frame is incomplete (partial read).
    #[error("Incomplete frame: expected {expected} bytes, got {actual}")]
    IncompleteFrame {
        /// Expected frame size.
        expected: usize,
        /// Actual bytes available.
        actual: usize,
    },

    /// Invalid stream ID (reserved values).
    #[error("Invalid stream ID: {stream_id} (reserved range: {reserved_start}-{reserved_end})")]
    InvalidStreamId {
        /// Invalid stream ID.
        stream_id: u32,
        /// Start of reserved range.
        reserved_start: u32,
        /// End of reserved range.
        reserved_end: u32,
    },

    /// Sequence number out of order.
    #[error("Sequence out of order: expected {expected}, got {actual}")]
    SequenceOutOfOrder {
        /// Expected sequence number.
        expected: u64,
        /// Actual sequence number.
        actual: u64,
    },

    /// Frame encoding error.
    #[error("Encoding error: {message}")]
    EncodingError {
        /// Error message.
        message: String,
    },

    /// Frame decoding error.
    #[error("Decoding error: {message}")]
    DecodingError {
        /// Error message.
        message: String,
    },

    /// I/O error during frame processing.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Memory allocation failure.
    #[error("Memory allocation failed: {message}")]
    AllocationError {
        /// Error message.
        message: String,
    },
}

/// Result type for protocol operations.
pub type ProtocolResult<T> = std::result::Result<T, ProtocolError>;

impl ProtocolError {
    /// Create an encoding error with a message.
    #[must_use]
    pub fn encoding(message: impl Into<String>) -> Self {
        Self::EncodingError { message: message.into() }
    }

    /// Create a decoding error with a message.
    #[must_use]
    pub fn decoding(message: impl Into<String>) -> Self {
        Self::DecodingError { message: message.into() }
    }

    /// Create an allocation error with a message.
    #[must_use]
    pub fn allocation(message: impl Into<String>) -> Self {
        Self::AllocationError { message: message.into() }
    }

    /// Check if this error indicates a recoverable condition.
    #[must_use]
    pub const fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::IncompleteFrame { .. }
                | Self::BufferTooSmall { .. }
                | Self::IoError(_)
                | Self::AllocationError { .. }
        )
    }

    /// Check if this error indicates frame corruption.
    #[must_use]
    pub const fn is_corruption(&self) -> bool {
        matches!(
            self,
            Self::InvalidMagic { .. }
                | Self::ChecksumMismatch { .. }
                | Self::InvalidFrameType { .. }
        )
    }

    /// Check if this error indicates a protocol violation.
    #[must_use]
    pub const fn is_protocol_violation(&self) -> bool {
        matches!(
            self,
            Self::UnsupportedVersion { .. }
                | Self::FrameTooLarge { .. }
                | Self::PayloadTooLarge { .. }
                | Self::InvalidStreamId { .. }
                | Self::SequenceOutOfOrder { .. }
        )
    }
}

// Manual implementation of Clone to handle IoError
impl Clone for ProtocolError {
    fn clone(&self) -> Self {
        match self {
            Self::InvalidMagic { expected, actual } => {
                Self::InvalidMagic { expected: *expected, actual: *actual }
            },
            Self::UnsupportedVersion { version, supported } => {
                Self::UnsupportedVersion { version: *version, supported: *supported }
            },
            Self::InvalidFrameType { frame_type } => {
                Self::InvalidFrameType { frame_type: *frame_type }
            },
            Self::FrameTooLarge { actual, max } => {
                Self::FrameTooLarge { actual: *actual, max: *max }
            },
            Self::PayloadTooLarge { actual, max } => {
                Self::PayloadTooLarge { actual: *actual, max: *max }
            },
            Self::BufferTooSmall { required, actual } => {
                Self::BufferTooSmall { required: *required, actual: *actual }
            },
            Self::ChecksumMismatch { expected, actual } => {
                Self::ChecksumMismatch { expected: *expected, actual: *actual }
            },
            Self::IncompleteFrame { expected, actual } => {
                Self::IncompleteFrame { expected: *expected, actual: *actual }
            },
            Self::InvalidStreamId { stream_id, reserved_start, reserved_end } => {
                Self::InvalidStreamId {
                    stream_id: *stream_id,
                    reserved_start: *reserved_start,
                    reserved_end: *reserved_end,
                }
            },
            Self::SequenceOutOfOrder { expected, actual } => {
                Self::SequenceOutOfOrder { expected: *expected, actual: *actual }
            },
            Self::EncodingError { message } => Self::EncodingError { message: message.clone() },
            Self::DecodingError { message } => Self::DecodingError { message: message.clone() },
            Self::IoError(e) => Self::IoError(std::io::Error::new(e.kind(), e.to_string())),
            Self::AllocationError { message } => Self::AllocationError { message: message.clone() },
        }
    }
}

// Manual implementation of PartialEq to handle IoError
#[allow(clippy::similar_names)]
impl PartialEq for ProtocolError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::InvalidMagic { expected: e1, actual: a1 },
                Self::InvalidMagic { expected: e2, actual: a2 },
            ) => e1 == e2 && a1 == a2,
            (
                Self::UnsupportedVersion { version: v1, supported: s1 },
                Self::UnsupportedVersion { version: v2, supported: s2 },
            ) => v1 == v2 && s1 == s2,
            (
                Self::InvalidFrameType { frame_type: ft1 },
                Self::InvalidFrameType { frame_type: ft2 },
            ) => ft1 == ft2,
            (
                Self::FrameTooLarge { actual: a1, max: m1 },
                Self::FrameTooLarge { actual: a2, max: m2 },
            ) => a1 == a2 && m1 == m2,
            (
                Self::PayloadTooLarge { actual: a1, max: m1 },
                Self::PayloadTooLarge { actual: a2, max: m2 },
            ) => a1 == a2 && m1 == m2,
            (
                Self::BufferTooSmall { required: r1, actual: a1 },
                Self::BufferTooSmall { required: r2, actual: a2 },
            ) => r1 == r2 && a1 == a2,
            (
                Self::ChecksumMismatch { expected: e1, actual: a1 },
                Self::ChecksumMismatch { expected: e2, actual: a2 },
            ) => e1 == e2 && a1 == a2,
            (
                Self::IncompleteFrame { expected: e1, actual: a1 },
                Self::IncompleteFrame { expected: e2, actual: a2 },
            ) => e1 == e2 && a1 == a2,
            (
                Self::InvalidStreamId { stream_id: s1, reserved_start: start1, reserved_end: end1 },
                Self::InvalidStreamId { stream_id: s2, reserved_start: start2, reserved_end: end2 },
            ) => s1 == s2 && start1 == start2 && end1 == end2,
            (
                Self::SequenceOutOfOrder { expected: e1, actual: a1 },
                Self::SequenceOutOfOrder { expected: e2, actual: a2 },
            ) => e1 == e2 && a1 == a2,
            (Self::EncodingError { message: m1 }, Self::EncodingError { message: m2 })
            | (Self::DecodingError { message: m1 }, Self::DecodingError { message: m2 })
            | (Self::AllocationError { message: m1 }, Self::AllocationError { message: m2 }) => {
                m1 == m2
            },
            (Self::IoError(e1), Self::IoError(e2)) => e1.kind() == e2.kind(),
            _ => false,
        }
    }
}

impl Eq for ProtocolError {}

// Integration with core error types
impl From<ProtocolError> for crate::Error {
    fn from(err: ProtocolError) -> Self {
        match err {
            ProtocolError::IoError(io_err) => Self::NetworkError(io_err.to_string()),
            _ => Self::Stream { message: err.to_string() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = ProtocolError::encoding("test message");
        assert_eq!(err.to_string(), "Encoding error: test message");
    }

    #[test]
    fn test_error_properties() {
        let incomplete = ProtocolError::IncompleteFrame { expected: 100, actual: 50 };
        assert!(incomplete.is_recoverable());
        assert!(!incomplete.is_corruption());
        assert!(!incomplete.is_protocol_violation());

        let checksum = ProtocolError::ChecksumMismatch { expected: 0x12345678, actual: 0x87654321 };
        assert!(!checksum.is_recoverable());
        assert!(checksum.is_corruption());
        assert!(!checksum.is_protocol_violation());

        let version = ProtocolError::UnsupportedVersion { version: 2, supported: 1 };
        assert!(!version.is_recoverable());
        assert!(!version.is_corruption());
        assert!(version.is_protocol_violation());
    }

    #[test]
    fn test_core_error_conversion() {
        let protocol_err = ProtocolError::encoding("test");
        let core_err: crate::Error = protocol_err.into();
        assert!(matches!(core_err, crate::Error::Stream { .. }));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "test");
        let protocol_err = ProtocolError::from(io_err);
        assert!(matches!(protocol_err, ProtocolError::IoError(_)));

        let core_err: crate::Error = protocol_err.into();
        assert!(matches!(core_err, crate::Error::NetworkError(_)));
    }

    #[test]
    fn test_error_cloning() {
        let original = ProtocolError::encoding("test message");
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_io_error_equality() {
        let err1 =
            ProtocolError::IoError(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "test1"));
        let err2 =
            ProtocolError::IoError(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "test2"));
        let err3 =
            ProtocolError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));

        assert_eq!(err1, err2); // Same error kind
        assert_ne!(err1, err3); // Different error kind
    }
}
