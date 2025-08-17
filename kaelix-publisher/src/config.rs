//! Publisher configuration types.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;

/// Configuration for the publisher client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherConfig {
    /// Broker addresses to connect to
    pub brokers: Vec<SocketAddr>,
    
    /// Connection configuration
    pub connection: ConnectionConfig,
    
    /// Batching configuration for performance
    pub batching: BatchConfig,
    
    /// Retry configuration
    pub retry: RetryConfig,
}

/// Connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Connection timeout
    pub timeout: Duration,
    
    /// Keep-alive interval
    pub keep_alive: Duration,
    
    /// Maximum number of connections per broker
    pub max_connections_per_broker: usize,
    
    /// Enable TLS
    pub tls_enabled: bool,
}

/// Batching configuration for improved performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Maximum batch size
    pub max_batch_size: usize,
    
    /// Maximum time to wait before sending a partial batch
    pub linger_ms: u64,
    
    /// Enable automatic batching
    pub auto_batch: bool,
}

/// Retry configuration for handling failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: usize,
    
    /// Initial retry delay
    pub initial_delay: Duration,
    
    /// Maximum retry delay
    pub max_delay: Duration,
    
    /// Backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for PublisherConfig {
    fn default() -> Self {
        Self {
            brokers: vec!["127.0.0.1:9092".parse().expect("Valid address")],
            connection: ConnectionConfig::default(),
            batching: BatchConfig::default(),
            retry: RetryConfig::default(),
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            keep_alive: Duration::from_secs(60),
            max_connections_per_broker: 5,
            tls_enabled: false,
        }
    }
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            linger_ms: 10,
            auto_batch: true,
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}