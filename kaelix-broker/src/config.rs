//! Broker configuration types and utilities.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Configuration for the broker instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    /// Network binding configuration
    pub network: NetworkConfig,
    
    /// Storage configuration
    pub storage: StorageConfig,
    
    /// Performance tuning parameters
    pub performance: PerformanceConfig,
    
    /// Security settings
    pub security: SecurityConfig,
}

/// Network configuration for client connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Address to bind for client connections
    pub bind_address: SocketAddr,
    
    /// Maximum number of concurrent connections
    pub max_connections: usize,
    
    /// Connection timeout
    pub connection_timeout: Duration,
    
    /// Enable TLS
    pub tls_enabled: bool,
}

/// Storage configuration for message persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Data directory for persistent storage
    pub data_dir: PathBuf,
    
    /// Maximum log file size
    pub max_log_size: u64,
    
    /// Log retention period
    pub retention_period: Duration,
    
    /// Enable compression
    pub compression_enabled: bool,
}

/// Performance tuning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Number of worker threads
    pub worker_threads: usize,
    
    /// Message batch size for processing
    pub batch_size: usize,
    
    /// Buffer sizes for various queues
    pub buffer_size: usize,
    
    /// Enable memory-mapped files
    pub mmap_enabled: bool,
}

/// Security configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable authentication
    pub auth_enabled: bool,
    
    /// TLS certificate path
    pub tls_cert_path: Option<PathBuf>,
    
    /// TLS private key path
    pub tls_key_path: Option<PathBuf>,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
            performance: PerformanceConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:9092".parse().expect("Valid address"),
            max_connections: 10000,
            connection_timeout: Duration::from_secs(30),
            tls_enabled: false,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            max_log_size: 1024 * 1024 * 1024, // 1GB
            retention_period: Duration::from_secs(7 * 24 * 3600), // 7 days
            compression_enabled: true,
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            worker_threads: num_cpus::get(),
            batch_size: 1000,
            buffer_size: 65536,
            mmap_enabled: true,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            auth_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}