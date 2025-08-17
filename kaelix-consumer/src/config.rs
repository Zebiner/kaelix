//! Consumer configuration types.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;

/// Configuration for the consumer client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerConfig {
    /// Broker addresses to connect to
    pub brokers: Vec<SocketAddr>,

    /// Consumer group ID
    pub group_id: String,

    /// Connection configuration
    pub connection: ConnectionConfig,

    /// Consumption configuration
    pub consumption: ConsumptionConfig,

    /// Offset management configuration
    pub offset: OffsetConfig,
}

/// Connection configuration for consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Connection timeout
    pub timeout: Duration,

    /// Session timeout for consumer group coordination
    pub session_timeout: Duration,

    /// Heartbeat interval
    pub heartbeat_interval: Duration,

    /// Enable TLS
    pub tls_enabled: bool,
}

/// Message consumption configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumptionConfig {
    /// Maximum number of messages to fetch in one request
    pub max_fetch_size: usize,

    /// Minimum bytes to fetch in one request
    pub min_fetch_bytes: usize,

    /// Maximum time to wait for fetch request
    pub fetch_timeout: Duration,

    /// Enable automatic processing
    pub auto_process: bool,
}

/// Offset management configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetConfig {
    /// Enable automatic offset commits
    pub auto_commit: bool,

    /// Interval for automatic offset commits
    pub auto_commit_interval: Duration,

    /// What to do when there's no initial offset
    pub reset_policy: OffsetResetPolicy,
}

/// Policy for handling missing offsets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OffsetResetPolicy {
    /// Start from the earliest available offset
    Earliest,
    /// Start from the latest offset
    Latest,
    /// Fail if no offset is found
    None,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            brokers: vec!["127.0.0.1:9092".parse().expect("Valid address")],
            group_id: "default-group".to_string(),
            connection: ConnectionConfig::default(),
            consumption: ConsumptionConfig::default(),
            offset: OffsetConfig::default(),
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            session_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(3),
            tls_enabled: false,
        }
    }
}

impl Default for ConsumptionConfig {
    fn default() -> Self {
        Self {
            max_fetch_size: 1000,
            min_fetch_bytes: 1024,
            fetch_timeout: Duration::from_millis(500),
            auto_process: true,
        }
    }
}

impl Default for OffsetConfig {
    fn default() -> Self {
        Self {
            auto_commit: true,
            auto_commit_interval: Duration::from_secs(5),
            reset_policy: OffsetResetPolicy::Latest,
        }
    }
}
