//! # Error Handling
//!
//! This module provides comprehensive error handling for the Kaelix Core library.

use thiserror::Error;

/// Core error types for the Kaelix streaming system
#[derive(Error, Debug)]
pub enum Error {
    /// Invalid input or parameters
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Operation cannot be performed in current state
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Timeout occurred during operation
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Network or communication related errors
    #[error("Network error: {0}")]
    Network(String),

    /// Network-related errors (alternative variant)
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Configuration loading/parsing errors
    #[error("Config error: {0}")]
    ConfigError(String),

    /// Configuration-related errors (additional variant)
    #[error("Configuration error: {0}")]
    Configuration(String),

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

    /// Resource exhausted error
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// Resource not found error
    #[error("Not found: {0}")]
    NotFound(String),

    /// I/O operation errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parsing errors
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    /// TOML serialization errors
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    /// Binary encoding/decoding errors
    #[error("Binary encoding error: {0}")]
    Bincode(#[from] bincode::Error),

    /// UUID parsing errors
    #[error("UUID error: {0}")]
    Uuid(#[from] uuid::Error),

    /// Time parsing/formatting errors
    #[error("Time error: {0}")]
    ChronoParseError(#[from] chrono::ParseError),

    /// File watcher errors (only available with hot-reload feature)
    #[cfg(feature = "hot-reload")]
    #[error("File watcher error: {0}")]
    Notify(#[from] notify::Error),

    /// Plugin-related errors
    #[error("Plugin error: {0}")]
    Plugin(String),

    /// Plugin loading errors
    #[error("Plugin loading error: {0}")]
    PluginLoading(String),

    /// Plugin execution errors
    #[error("Plugin execution error: {0}")]
    PluginExecution(String),

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Shutdown signal received
    #[error("Shutdown requested")]
    Shutdown,

    /// Memory allocation errors
    #[error("Memory allocation error: {0}")]
    Memory(String),

    /// Thread pool errors
    #[error("Thread pool error: {0}")]
    ThreadPool(String),

    /// Channel communication errors
    #[error("Channel error: {0}")]
    Channel(String),

    /// Lock acquisition errors
    #[error("Lock error: {0}")]
    Lock(String),

    /// Database or storage errors
    #[error("Storage error: {0}")]
    Storage(String),

    /// Authentication/authorization errors
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Rate limiting errors
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Backpressure applied
    #[error("Backpressure applied: {0}")]
    Backpressure(String),

    /// Circuit breaker errors
    #[error("Circuit breaker open: {0}")]
    CircuitBreaker(String),

    /// Health check failures
    #[error("Health check failed: {0}")]
    HealthCheck(String),

    /// Metrics collection errors
    #[error("Metrics error: {0}")]
    Metrics(String),

    /// Tracing/observability errors
    #[error("Tracing error: {0}")]
    Tracing(String),

    /// Feature not implemented
    #[error("Feature not implemented: {0}")]
    NotImplemented(String),

    /// Feature not available due to missing dependencies
    #[error("Feature not available: {0}")]
    FeatureNotAvailable(String),

    /// Version compatibility errors
    #[error("Version incompatible: {0}")]
    VersionIncompatible(String),

    /// Data corruption detected
    #[error("Data corruption: {0}")]
    DataCorruption(String),

    /// Checksum validation failed
    #[error("Checksum mismatch: {0}")]
    ChecksumMismatch(String),

    /// Encryption/decryption errors
    #[error("Crypto error: {0}")]
    Crypto(String),

    /// Compression/decompression errors
    #[error("Compression error: {0}")]
    Compression(String),

    /// Protocol errors
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Message format errors
    #[error("Message format error: {0}")]
    MessageFormat(String),

    /// Partition errors
    #[error("Partition error: {0}")]
    Partition(String),

    /// Offset management errors
    #[error("Offset error: {0}")]
    Offset(String),

    /// Consumer group errors
    #[error("Consumer group error: {0}")]
    ConsumerGroup(String),

    /// Producer errors
    #[error("Producer error: {0}")]
    Producer(String),

    /// Consumer errors
    #[error("Consumer error: {0}")]
    Consumer(String),

    /// Topic management errors
    #[error("Topic error: {0}")]
    Topic(String),

    /// Subscription errors
    #[error("Subscription error: {0}")]
    Subscription(String),

    /// Message delivery errors
    #[error("Delivery error: {0}")]
    Delivery(String),

    /// Acknowledgment errors
    #[error("Acknowledgment error: {0}")]
    Acknowledgment(String),

    /// Replication errors
    #[error("Replication error: {0}")]
    Replication(String),

    /// Leadership election errors
    #[error("Leadership error: {0}")]
    Leadership(String),

    /// Cluster membership errors
    #[error("Cluster error: {0}")]
    Cluster(String),

    /// Node communication errors
    #[error("Node communication error: {0}")]
    NodeCommunication(String),

    /// Consensus errors
    #[error("Consensus error: {0}")]
    Consensus(String),

    /// State synchronization errors
    #[error("State sync error: {0}")]
    StateSync(String),

    /// Migration errors
    #[error("Migration error: {0}")]
    Migration(String),

    /// Recovery errors
    #[error("Recovery error: {0}")]
    Recovery(String),

    /// Backup/restore errors
    #[error("Backup error: {0}")]
    Backup(String),
}

/// Convert validation errors to configuration errors
impl From<validator::ValidationErrors> for Error {
    fn from(err: validator::ValidationErrors) -> Self {
        Self::Configuration(format!("Validation failed: {err}"))
    }
}

impl Error {
    /// Create a new configuration error
    pub fn config<T: std::fmt::Display>(message: T) -> Self {
        Self::Configuration(message.to_string())
    }

    /// Create a new runtime error
    pub fn runtime<T: std::fmt::Display>(message: T) -> Self {
        Self::Runtime(message.to_string())
    }

    /// Create a new invalid input error
    pub fn invalid_input<T: std::fmt::Display>(message: T) -> Self {
        Self::InvalidInput(message.to_string())
    }

    /// Create a new timeout error
    pub fn timeout<T: std::fmt::Display>(message: T) -> Self {
        Self::Timeout(message.to_string())
    }

    /// Check if this is a retryable error
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_)
                | Self::NetworkError(_)
                | Self::Timeout(_)
                | Self::Io(_)
                | Self::ResourceExhausted(_)
                | Self::Backpressure(_)
                | Self::CircuitBreaker(_)
        )
    }

    /// Check if this is a fatal error that should cause shutdown
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::DataCorruption(_) | Self::Memory(_) | Self::Internal(_) | Self::Shutdown
        )
    }

    /// Get the error category for metrics and logging
    pub fn category(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) | Self::InvalidOperation(_) => "validation",
            Self::Timeout(_) => "timeout",
            Self::Network(_) | Self::NetworkError(_) | Self::NodeCommunication(_) => "network",
            Self::Serialization(_) | Self::Json(_) | Self::Toml(_) | Self::TomlSer(_) => {
                "serialization"
            },
            Self::Configuration(_) | Self::ConfigError(_) => "configuration",
            Self::Runtime(_) => "runtime",
            Self::ResourceLimit { .. } | Self::ResourceExhausted(_) => "resource",
            Self::NotFound(_) => "not_found",
            Self::Io(_) => "io",
            Self::Bincode(_) => "encoding",
            Self::Uuid(_) => "uuid",
            Self::ChronoParseError(_) => "time",
            #[cfg(feature = "hot-reload")]
            Self::Notify(_) => "file_watcher",
            Self::Plugin(_) | Self::PluginLoading(_) | Self::PluginExecution(_) => "plugin",
            Self::Internal(_) => "internal",
            Self::Shutdown => "shutdown",
            Self::Memory(_) => "memory",
            Self::ThreadPool(_) => "thread_pool",
            Self::Channel(_) => "channel",
            Self::Lock(_) => "lock",
            Self::Storage(_) => "storage",
            Self::Auth(_) => "auth",
            Self::RateLimit(_) => "rate_limit",
            Self::Backpressure(_) => "backpressure",
            Self::CircuitBreaker(_) => "circuit_breaker",
            Self::HealthCheck(_) => "health_check",
            Self::Metrics(_) => "metrics",
            Self::Tracing(_) => "tracing",
            Self::NotImplemented(_) => "not_implemented",
            Self::FeatureNotAvailable(_) => "feature_not_available",
            Self::VersionIncompatible(_) => "version",
            Self::DataCorruption(_) => "data_corruption",
            Self::ChecksumMismatch(_) => "checksum",
            Self::Crypto(_) => "crypto",
            Self::Compression(_) => "compression",
            Self::Protocol(_) => "protocol",
            Self::MessageFormat(_) => "message_format",
            Self::Partition(_) => "partition",
            Self::Offset(_) => "offset",
            Self::ConsumerGroup(_) => "consumer_group",
            Self::Producer(_) => "producer",
            Self::Consumer(_) => "consumer",
            Self::Topic(_) => "topic",
            Self::Subscription(_) => "subscription",
            Self::Delivery(_) => "delivery",
            Self::Acknowledgment(_) => "acknowledgment",
            Self::Replication(_) => "replication",
            Self::Leadership(_) => "leadership",
            Self::Cluster(_) => "cluster",
            Self::Consensus(_) => "consensus",
            Self::StateSync(_) => "state_sync",
            Self::Migration(_) => "migration",
            Self::Recovery(_) => "recovery",
            Self::Backup(_) => "backup",
        }
    }
}

/// A specialized `Result` type for Kaelix operations
pub type Result<T> = std::result::Result<T, Error>;