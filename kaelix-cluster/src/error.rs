//! # Cluster Error Handling
//!
//! This module provides comprehensive error handling for distributed cluster operations.

use thiserror::Error;

/// Cluster-specific error types for the Kaelix distributed system
#[derive(Error, Debug)]
pub enum Error {
    /// Cluster membership errors
    #[error("Membership error: {0}")]
    Membership(String),

    /// Node discovery failures
    #[error("Node discovery failed: {0}")]
    NodeDiscovery(String),

    /// Failure detection errors
    #[error("Failure detection error: {0}")]
    FailureDetection(String),

    /// Consensus algorithm errors
    #[error("Consensus error: {0}")]
    Consensus(String),

    /// Leader election failures
    #[error("Leader election failed: {0}")]
    LeaderElection(String),

    /// Log replication errors
    #[error("Log replication error: {0}")]
    LogReplication(String),

    /// State machine errors
    #[error("State machine error: {0}")]
    StateMachine(String),

    /// Inter-node communication failures
    #[error("Communication error: {0}")]
    Communication(String),

    /// Network transport errors
    #[error("Network transport error: {0}")]
    NetworkTransport(String),

    /// Message routing failures
    #[error("Message routing error: {0}")]
    MessageRouting(String),

    /// Connection management errors
    #[error("Connection error: {0}")]
    Connection(String),

    /// Cluster configuration errors
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Node configuration validation
    #[error("Node configuration invalid: {0}")]
    NodeConfiguration(String),

    /// Cluster bootstrap failures
    #[error("Bootstrap error: {0}")]
    Bootstrap(String),

    /// Join cluster operation failures
    #[error("Join cluster failed: {0}")]
    JoinCluster(String),

    /// Leave cluster operation failures
    #[error("Leave cluster failed: {0}")]
    LeaveCluster(String),

    /// Split-brain detection
    #[error("Split-brain detected: {0}")]
    SplitBrain(String),

    /// Quorum loss errors
    #[error("Quorum lost: {0}")]
    QuorumLoss(String),

    /// Node unavailable
    #[error("Node unavailable: {node_id}")]
    NodeUnavailable {
        /// ID of the unavailable node
        node_id: String,
    },

    /// Cluster partition errors
    #[error("Cluster partition detected: {0}")]
    ClusterPartition(String),

    /// Timeout waiting for cluster operations
    #[error("Cluster operation timeout: {operation} after {timeout_ms}ms")]
    ClusterTimeout {
        /// Name of the operation that timed out
        operation: String,
        /// Timeout duration in milliseconds
        timeout_ms: u64,
    },

    /// Invalid node state for operation
    #[error("Invalid node state: expected {expected}, got {actual}")]
    InvalidNodeState {
        /// Expected node state
        expected: String,
        /// Actual node state
        actual: String,
    },

    /// Term mismatch in consensus
    #[error("Term mismatch: expected {expected}, got {actual}")]
    TermMismatch {
        /// Expected term
        expected: u64,
        /// Actual term
        actual: u64,
    },

    /// Index mismatch in log replication
    #[error("Index mismatch: expected {expected}, got {actual}")]
    IndexMismatch {
        /// Expected log index
        expected: u64,
        /// Actual log index
        actual: u64,
    },

    /// Incompatible protocol versions
    #[error("Protocol version mismatch: node {node_version}, cluster {cluster_version}")]
    ProtocolMismatch {
        /// Node protocol version
        node_version: String,
        /// Cluster protocol version
        cluster_version: String,
    },

    /// Security and authentication errors
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Authorization errors
    #[error("Authorization denied: {0}")]
    Authorization(String),

    /// Certificate validation errors
    #[error("Certificate validation failed: {0}")]
    CertificateValidation(String),

    /// Encryption/decryption errors
    #[error("Encryption error: {0}")]
    Encryption(String),

    /// Message integrity check failures
    #[error("Message integrity check failed: {0}")]
    MessageIntegrity(String),

    /// Storage and persistence errors
    #[error("Storage error: {0}")]
    Storage(String),

    /// Snapshot creation/restoration errors
    #[error("Snapshot error: {0}")]
    Snapshot(String),

    /// Log compaction errors
    #[error("Log compaction error: {0}")]
    LogCompaction(String),

    /// Backup/restore operations
    #[error("Backup operation failed: {0}")]
    Backup(String),

    /// Recovery operations
    #[error("Recovery failed: {0}")]
    Recovery(String),

    /// Resource exhaustion
    #[error("Resource exhausted: {resource} (limit: {limit})")]
    ResourceExhausted {
        /// Name of the exhausted resource
        resource: String,
        /// Resource limit that was hit
        limit: String,
    },

    /// Memory allocation failures
    #[error("Memory allocation failed: {0}")]
    Memory(String),

    /// Thread pool saturation
    #[error("Thread pool saturated: {0}")]
    ThreadPoolSaturated(String),

    /// Channel capacity exceeded
    #[error("Channel capacity exceeded: {0}")]
    ChannelCapacityExceeded(String),

    /// Backpressure applied
    #[error("Backpressure applied: {0}")]
    Backpressure(String),

    /// Rate limiting errors
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Circuit breaker errors
    #[error("Circuit breaker open: {service}")]
    CircuitBreaker {
        /// Name of the service with circuit breaker open
        service: String,
    },

    /// Health check failures
    #[error("Health check failed: {check}")]
    HealthCheck {
        /// Name of the failed health check
        check: String,
    },

    /// Metrics collection errors
    #[error("Metrics collection failed: {0}")]
    Metrics(String),

    /// Tracing/observability errors
    #[error("Tracing error: {0}")]
    Tracing(String),

    /// Feature not implemented
    #[error("Feature not implemented: {0}")]
    NotImplemented(String),

    /// Feature disabled or unavailable
    #[error("Feature unavailable: {0}")]
    FeatureUnavailable(String),

    /// Internal system errors
    #[error("Internal error: {0}")]
    Internal(String),

    /// Conversion from core kaelix errors
    #[error("Core error: {0}")]
    Core(#[from] kaelix_core::Error),

    /// Standard I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Binary encoding errors
    #[error("Binary encoding error: {0}")]
    Bincode(#[from] bincode::Error),

    /// UUID parsing errors
    #[error("UUID error: {0}")]
    Uuid(#[from] uuid::Error),

    /// Time parsing errors
    #[error("Time parsing error: {0}")]
    ChronoParseError(#[from] chrono::ParseError),

    /// Configuration validation errors
    #[error("Validation error: {0}")]
    Validation(#[from] validator::ValidationErrors),

    /// TOML parsing errors
    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),

    /// Consensus-specific errors from the consensus module
    #[error("Consensus subsystem error: {0}")]
    ConsensusSubsystem(#[from] crate::consensus::ConsensusError),
}

impl Error {
    /// Create a new membership error
    pub fn membership<T: std::fmt::Display>(message: T) -> Self {
        Self::Membership(message.to_string())
    }

    /// Create a new consensus error
    pub fn consensus<T: std::fmt::Display>(message: T) -> Self {
        Self::Consensus(message.to_string())
    }

    /// Create a new communication error
    pub fn communication<T: std::fmt::Display>(message: T) -> Self {
        Self::Communication(message.to_string())
    }

    /// Create a new configuration error
    pub fn configuration<T: std::fmt::Display>(message: T) -> Self {
        Self::Configuration(message.to_string())
    }

    /// Create a cluster timeout error
    pub fn cluster_timeout<T: std::fmt::Display>(operation: T, timeout_ms: u64) -> Self {
        Self::ClusterTimeout { operation: operation.to_string(), timeout_ms }
    }

    /// Create a node unavailable error
    pub fn node_unavailable<T: std::fmt::Display>(node_id: T) -> Self {
        Self::NodeUnavailable { node_id: node_id.to_string() }
    }

    /// Check if this error indicates a temporary condition that may be retried
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Communication(_)
                | Self::NetworkTransport(_)
                | Self::Connection(_)
                | Self::NodeUnavailable { .. }
                | Self::ClusterTimeout { .. }
                | Self::QuorumLoss(_)
                | Self::Backpressure(_)
                | Self::RateLimit(_)
                | Self::CircuitBreaker { .. }
                | Self::ResourceExhausted { .. }
                | Self::ThreadPoolSaturated(_)
                | Self::ChannelCapacityExceeded(_)
                | Self::Core(kaelix_core::Error::Network(_))
                | Self::Core(kaelix_core::Error::Timeout(_))
                | Self::Core(kaelix_core::Error::ResourceExhausted(_))
                | Self::Io(_)
        )
    }

    /// Check if this error should trigger cluster failover
    pub fn requires_failover(&self) -> bool {
        matches!(
            self,
            Self::SplitBrain(_)
                | Self::QuorumLoss(_)
                | Self::ClusterPartition(_)
                | Self::LeaderElection(_)
                | Self::NodeUnavailable { .. }
                | Self::Core(kaelix_core::Error::DataCorruption(_))
                | Self::Internal(_)
        )
    }

    /// Check if this error is fatal and requires immediate shutdown
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::Memory(_)
                | Self::Internal(_)
                | Self::Core(kaelix_core::Error::Shutdown)
                | Self::Core(kaelix_core::Error::Memory(_))
                | Self::Core(kaelix_core::Error::DataCorruption(_))
        )
    }

    /// Get the error category for metrics and logging
    pub fn category(&self) -> &'static str {
        match self {
            Self::Membership(_) | Self::NodeDiscovery(_) | Self::FailureDetection(_) => {
                "membership"
            },
            Self::Consensus(_)
            | Self::LeaderElection(_)
            | Self::LogReplication(_)
            | Self::StateMachine(_)
            | Self::ConsensusSubsystem(_) => "consensus",
            Self::Communication(_)
            | Self::NetworkTransport(_)
            | Self::MessageRouting(_)
            | Self::Connection(_) => "communication",
            Self::Configuration(_) | Self::NodeConfiguration(_) => "configuration",
            Self::Bootstrap(_) | Self::JoinCluster(_) | Self::LeaveCluster(_) => "cluster_ops",
            Self::SplitBrain(_) | Self::QuorumLoss(_) | Self::ClusterPartition(_) => {
                "cluster_health"
            },
            Self::NodeUnavailable { .. }
            | Self::ClusterTimeout { .. }
            | Self::InvalidNodeState { .. } => "node_state",
            Self::TermMismatch { .. }
            | Self::IndexMismatch { .. }
            | Self::ProtocolMismatch { .. } => "protocol",
            Self::Authentication(_)
            | Self::Authorization(_)
            | Self::CertificateValidation(_)
            | Self::Encryption(_)
            | Self::MessageIntegrity(_) => "security",
            Self::Storage(_)
            | Self::Snapshot(_)
            | Self::LogCompaction(_)
            | Self::Backup(_)
            | Self::Recovery(_) => "storage",
            Self::ResourceExhausted { .. }
            | Self::Memory(_)
            | Self::ThreadPoolSaturated(_)
            | Self::ChannelCapacityExceeded(_) => "resources",
            Self::Backpressure(_) | Self::RateLimit(_) | Self::CircuitBreaker { .. } => {
                "flow_control"
            },
            Self::HealthCheck { .. } | Self::Metrics(_) | Self::Tracing(_) => "observability",
            Self::NotImplemented(_) | Self::FeatureUnavailable(_) => "features",
            Self::Internal(_) => "internal",
            Self::Core(_) => "core",
            Self::Io(_) => "io",
            Self::Json(_) | Self::Bincode(_) => "serialization",
            Self::Uuid(_) => "uuid",
            Self::ChronoParseError(_) => "time",
            Self::Validation(_) => "validation",
            Self::Toml(_) => "config_parsing",
        }
    }

    /// Get the severity level of this error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            // Critical errors that require immediate attention
            Self::SplitBrain(_) | Self::QuorumLoss(_) | Self::Memory(_) | Self::Internal(_) => {
                ErrorSeverity::Critical
            },

            // High severity errors that affect cluster operations
            Self::LeaderElection(_)
            | Self::ClusterPartition(_)
            | Self::Authentication(_)
            | Self::Authorization(_) => ErrorSeverity::High,

            // Medium severity errors that may impact performance
            Self::NodeUnavailable { .. }
            | Self::Communication(_)
            | Self::ResourceExhausted { .. }
            | Self::CircuitBreaker { .. } => ErrorSeverity::Medium,

            // Low severity errors that are generally recoverable
            Self::ClusterTimeout { .. } | Self::Backpressure(_) | Self::RateLimit(_) => {
                ErrorSeverity::Low
            },

            // Informational errors
            Self::NotImplemented(_) | Self::FeatureUnavailable(_) => ErrorSeverity::Info,

            // Default to medium for other errors including consensus subsystem errors
            _ => ErrorSeverity::Medium,
        }
    }
}

/// Error severity levels for categorization and alerting
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Informational - no action required
    Info,
    /// Low severity - monitoring recommended
    Low,
    /// Medium severity - investigation recommended
    Medium,
    /// High severity - immediate investigation required
    High,
    /// Critical severity - immediate action required
    Critical,
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::High => write!(f, "HIGH"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A specialized `Result` type for cluster operations
pub type Result<T> = std::result::Result<T, Error>;