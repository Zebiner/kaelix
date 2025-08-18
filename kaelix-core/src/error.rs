//! Error types for the Kaelix core library.

use thiserror::Error;

/// Main error type for Kaelix core operations.
#[derive(Error, Debug, Clone)]
pub enum Error {
    /// Invalid message format or content
    #[error("Invalid message: {message}")]
    InvalidMessage {
        /// Error message describing what was invalid
        message: String,
    },

    /// Serialization/deserialization errors
    #[error("Serialization error: {message}")]
    Serialization {
        /// Error message describing the serialization issue
        message: String,
    },

    /// Stream processing errors
    #[error("Stream error: {message}")]
    Stream {
        /// Error message describing the stream issue
        message: String,
    },

    /// Network-related errors
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Runtime-related errors
    #[error("Runtime error: {0}")]
    Runtime(String),

    /// Resource limits exceeded
    #[error("Resource limit exceeded: {resource} ({limit})")]
    ResourceLimit {
        /// Name of the resource that exceeded its limit
        resource: String,
        /// The limit that was exceeded
        limit: String,
    },

    /// Configuration errors (legacy format - kept for compatibility)
    #[error("Configuration error: {message}")]
    Configuration {
        /// Error message describing the configuration issue
        message: String,
    },

    /// Internal system errors
    #[error("Internal error: {message}")]
    Internal {
        /// Error message describing the internal issue
        message: String,
    },
}

/// Result type alias for Kaelix operations.
pub type Result<T> = std::result::Result<T, Error>;

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization { message: err.to_string() }
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Self::Serialization { message: err.to_string() }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::NetworkError(err.to_string())
    }
}