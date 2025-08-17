//! Error types for the Kaelix core library.

use thiserror::Error;

/// Main error type for Kaelix core operations.
#[derive(Error, Debug, Clone)]
pub enum Error {
    /// Invalid message format or content
    #[error("Invalid message: {message}")]
    InvalidMessage { message: String },

    /// Serialization/deserialization errors
    #[error("Serialization error: {message}")]
    Serialization { message: String },

    /// Stream processing errors
    #[error("Stream error: {message}")]
    Stream { message: String },

    /// Resource limits exceeded
    #[error("Resource limit exceeded: {resource} ({limit})")]
    ResourceLimit { resource: String, limit: String },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// Internal system errors
    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Result type alias for Kaelix operations.
pub type Result<T> = std::result::Result<T, Error>;

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Serialization {
            message: err.to_string(),
        }
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Error::Serialization {
            message: err.to_string(),
        }
    }
}