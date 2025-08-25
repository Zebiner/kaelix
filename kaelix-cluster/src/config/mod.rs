//! # Cluster Configuration
//!
//! This module provides configuration management for distributed cluster operations.

use crate::{
    error::{Error, Result},
    types::NodeId,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, net::SocketAddr, path::PathBuf, time::Duration};
use validator::Validate;

/// Comprehensive cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ClusterConfig {
    /// Unique identifier for this node
    pub node_id: NodeId,

    /// Address to bind for incoming cluster connections
    pub bind_address: SocketAddr,

    /// Human-readable cluster name
    #[validate(length(min = 1, max = 128))]
    pub cluster_name: String,

    /// Initial seed nodes for cluster discovery
    pub seed_nodes: Vec<SocketAddr>,

    /// Membership configuration
    pub membership: MembershipConfig,

    /// Consensus configuration
    pub consensus: ConsensusConfig,

    /// Communication configuration
    pub communication: CommunicationConfig,

    /// Security configuration
    pub security: SecurityConfig,

    /// Performance tuning configuration
    pub performance: PerformanceConfig,
    
    /// High availability configuration
    pub ha: HaConfigCluster,
}

impl ClusterConfig {
    /// Create a new configuration builder
    #[must_use]
    pub fn builder() -> ClusterConfigBuilder {
        ClusterConfigBuilder::default()
    }

    /// Get the node ID
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the bind address
    #[must_use]
    pub const fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }

    /// Get the cluster name
    #[must_use]
    pub fn cluster_name(&self) -> &str {
        &self.cluster_name
    }

    /// Validate the configuration
    pub fn validate_config(&self) -> Result<()> {
        self.validate()
            .map_err(|e| Error::Configuration(format!("Configuration validation failed: {e}")))?;

        // Additional custom validations
        if self.bind_address.port() == 0 {
            return Err(Error::Configuration("Bind address port cannot be 0".to_string()));
        }

        let (min, max) = self.consensus.election_timeout_ms;
        if min >= max {
            return Err(Error::Configuration(
                "Min election timeout must be less than max timeout".to_string(),
            ));
        }
        if min < 150 || max > 10000 {
            return Err(Error::Configuration(
                "Election timeout must be between 150ms and 10000ms".to_string(),
            ));
        }

        Ok(())
    }
}

/// Node-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct NodeConfig {
    /// Node role in the cluster
    pub role: NodeRole,
    /// Node capabilities
    pub capabilities: Vec<String>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            role: NodeRole::Member,
            capabilities: vec!["storage".to_string(), "compute".to_string()],
        }
    }
}

/// Node role in the cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    /// Bootstrap node (initial cluster formation)
    Bootstrap,
    /// Regular cluster member
    Member,
    /// Observer node (read-only)
    Observer,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct NetworkConfig {
    /// Network interface to bind to
    pub interface: String,
    /// Network timeout for operations
    #[validate(range(min = 100, max = 30000))]
    pub timeout_ms: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            interface: "0.0.0.0".to_string(),
            timeout_ms: 5000,
        }
    }
}

/// High availability configuration (for cluster config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaConfigCluster {
    /// Enable automatic failover
    pub enable_auto_failover: bool,
    /// Failover timeout
    pub failover_timeout_ms: u64,
    /// Enable health monitoring
    pub enable_health_monitoring: bool,
}

impl Default for HaConfigCluster {
    fn default() -> Self {
        Self {
            enable_auto_failover: true,
            failover_timeout_ms: 30000,
            enable_health_monitoring: true,
        }
    }
}

/// High availability configuration  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaConfig {
    /// Enable automatic failover
    pub enable_auto_failover: bool,
    /// Failover timeout
    pub failover_timeout_ms: u64,
}

impl Default for HaConfig {
    fn default() -> Self {
        Self {
            enable_auto_failover: true,
            failover_timeout_ms: 30000,
        }
    }
}

/// Telemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Enable metrics collection
    pub enable_metrics: bool,
    /// Metrics endpoint
    pub metrics_endpoint: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            metrics_endpoint: "127.0.0.1:9090".to_string(),
        }
    }
}

/// Membership protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MembershipConfig {
    /// Membership protocol to use
    pub protocol: MembershipProtocol,

    /// Failure detection timeout
    #[validate(range(min = 100, max = 30000))]
    pub failure_timeout_ms: u64,

    /// Heartbeat interval
    #[validate(range(min = 50, max = 10000))]
    pub heartbeat_interval_ms: u64,

    /// Maximum number of nodes to track
    #[validate(range(min = 1, max = 100000))]
    pub max_nodes: usize,

    /// Gossip fanout factor
    #[validate(range(min = 1, max = 20))]
    pub gossip_fanout: usize,

    /// Suspicion multiplier for SWIM protocol
    #[validate(range(min = 1, max = 10))]
    pub suspicion_multiplier: u32,
}

impl Default for MembershipConfig {
    fn default() -> Self {
        Self {
            protocol: MembershipProtocol::Swim,
            failure_timeout_ms: 5000,
            heartbeat_interval_ms: 1000,
            max_nodes: 10000,
            gossip_fanout: 3,
            suspicion_multiplier: 3,
        }
    }
}

/// Supported membership protocols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipProtocol {
    /// SWIM protocol for scalable membership
    Swim,
    /// Rapid failure detection protocol
    Rapid,
    /// Hybrid approach combining SWIM and gossip
    Hybrid,
}

/// Consensus algorithm configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConsensusConfig {
    /// Consensus algorithm to use
    pub algorithm: ConsensusAlgorithm,

    /// Election timeout range (min, max) in milliseconds
    pub election_timeout_ms: (u64, u64),

    /// Heartbeat interval for leader
    #[validate(range(min = 10, max = 5000))]
    pub heartbeat_interval_ms: u64,

    /// Maximum log entries per append request
    #[validate(range(min = 1, max = 10000))]
    pub max_entries_per_request: usize,

    /// Log compaction threshold
    #[validate(range(min = 100, max = 1000000))]
    pub log_compaction_threshold: u64,

    /// Snapshot threshold
    #[validate(range(min = 1000, max = 10000000))]
    pub snapshot_threshold: u64,

    /// Pre-vote optimization
    pub enable_pre_vote: bool,

    /// Pipelining optimization
    pub enable_pipelining: bool,

    /// Enable consensus (needed for lib.rs)
    pub enabled: bool,

    /// Cluster size for consensus
    pub cluster_size: usize,

    /// Consensus algorithm name for matching
    pub algorithm_name: String,

    /// Raft-specific configuration
    pub raft: RaftConfig,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            algorithm: ConsensusAlgorithm::Raft,
            election_timeout_ms: (1500, 3000),
            heartbeat_interval_ms: 500,
            max_entries_per_request: 1000,
            log_compaction_threshold: 10000,
            snapshot_threshold: 100000,
            enable_pre_vote: true,
            enable_pipelining: true,
            enabled: true,
            cluster_size: 3,
            algorithm_name: "raft".to_string(),
            raft: RaftConfig::default(),
        }
    }
}

/// Raft-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    /// Enable leadership lease
    pub enable_leadership_lease: bool,
    /// Leadership lease timeout
    pub leadership_lease_timeout_ms: u64,
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            enable_leadership_lease: true,
            leadership_lease_timeout_ms: 5000,
        }
    }
}

/// Supported consensus algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusAlgorithm {
    /// Standard Raft consensus
    Raft,
    /// Fast Raft with optimizations
    FastRaft,
    /// Pipeline Raft for high throughput
    PipelineRaft,
    /// Multi-Raft for partitioned data
    MultiRaft,
}

/// Communication and networking configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CommunicationConfig {
    /// Maximum concurrent connections per node
    #[validate(range(min = 1, max = 100000))]
    pub max_connections: usize,

    /// Connection timeout in milliseconds
    #[validate(range(min = 100, max = 30000))]
    pub connection_timeout_ms: u64,

    /// Keep-alive interval for connections
    #[validate(range(min = 1000, max = 60000))]
    pub keepalive_interval_ms: u64,

    /// Maximum message size in bytes
    #[validate(range(min = 1024, max = 134217728))] // 1KB to 128MB
    pub max_message_size: usize,

    /// Send buffer size
    #[validate(range(min = 4096, max = 16777216))] // 4KB to 16MB
    pub send_buffer_size: usize,

    /// Receive buffer size
    #[validate(range(min = 4096, max = 16777216))] // 4KB to 16MB
    pub recv_buffer_size: usize,

    /// Use QUIC transport
    pub enable_quic: bool,

    /// Enable zero-copy optimizations
    pub enable_zero_copy: bool,

    /// Compression algorithm
    pub compression: CompressionAlgorithm,
}

impl Default for CommunicationConfig {
    fn default() -> Self {
        Self {
            max_connections: 10000,
            connection_timeout_ms: 5000,
            keepalive_interval_ms: 30000,
            max_message_size: 16 * 1024 * 1024, // 16MB
            send_buffer_size: 64 * 1024,        // 64KB
            recv_buffer_size: 64 * 1024,        // 64KB
            enable_quic: true,
            enable_zero_copy: true,
            compression: CompressionAlgorithm::Lz4,
        }
    }
}

/// Supported compression algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// LZ4 compression (fast)
    Lz4,
    /// Zstandard compression (balanced)
    Zstd,
    /// Gzip compression (high ratio)
    Gzip,
}

/// Security and encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SecurityConfig {
    /// Enable TLS encryption
    pub enable_tls: bool,

    /// TLS certificate file path
    pub cert_path: Option<PathBuf>,

    /// TLS private key file path
    pub key_path: Option<PathBuf>,

    /// CA certificate file path for verification
    pub ca_path: Option<PathBuf>,

    /// Require client certificate authentication
    pub require_client_cert: bool,

    /// Enable message authentication codes
    pub enable_message_auth: bool,

    /// Allowed cipher suites
    pub cipher_suites: Vec<String>,

    /// Minimum TLS version
    pub min_tls_version: TlsVersion,

    /// Node authentication token (optional)
    pub auth_token: Option<String>,

    /// Trusted node IDs (empty means trust all)
    pub trusted_nodes: HashSet<NodeId>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_tls: true,
            cert_path: None,
            key_path: None,
            ca_path: None,
            require_client_cert: false,
            enable_message_auth: true,
            cipher_suites: vec![
                "TLS_AES_256_GCM_SHA384".to_string(),
                "TLS_CHACHA20_POLY1305_SHA256".to_string(),
                "TLS_AES_128_GCM_SHA256".to_string(),
            ],
            min_tls_version: TlsVersion::V1_3,
            auth_token: None,
            trusted_nodes: HashSet::new(),
        }
    }
}

/// Supported TLS versions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlsVersion {
    /// TLS 1.2
    V1_2,
    /// TLS 1.3
    V1_3,
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PerformanceConfig {
    /// Worker thread pool size (0 = auto-detect)
    pub worker_threads: usize,

    /// I/O thread pool size (0 = auto-detect)
    pub io_threads: usize,

    /// Enable NUMA awareness
    pub numa_aware: bool,

    /// CPU affinity settings
    pub cpu_affinity: Vec<usize>,

    /// Memory allocator to use
    pub allocator: Allocator,

    /// Enable memory pre-allocation
    pub pre_allocate_memory: bool,

    /// Memory pool size in bytes (1MB to 64GB max)
    #[validate(range(min = 1048576, max = 68719476736_usize))]
    pub memory_pool_size: usize,

    /// Enable lock-free data structures
    pub enable_lock_free: bool,

    /// Batch processing size
    #[validate(range(min = 1, max = 10000))]
    pub batch_size: usize,

    /// Enable metrics collection
    pub enable_metrics: bool,

    /// Metrics collection interval
    #[validate(range(min = 100, max = 60000))]
    pub metrics_interval_ms: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            worker_threads: 0, // Auto-detect
            io_threads: 0,     // Auto-detect
            numa_aware: true,
            cpu_affinity: Vec::new(),
            allocator: Allocator::System,
            pre_allocate_memory: true,
            memory_pool_size: 128 * 1024 * 1024, // 128MB
            enable_lock_free: true,
            batch_size: 100,
            enable_metrics: true,
            metrics_interval_ms: 5000,
        }
    }
}

/// Memory allocator options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Allocator {
    /// System default allocator
    System,
    /// jemalloc allocator
    Jemalloc,
    /// mimalloc allocator
    Mimalloc,
}

/// Configuration builder for fluent API
#[derive(Debug, Default)]
pub struct ClusterConfigBuilder {
    node_id: Option<NodeId>,
    bind_address: Option<SocketAddr>,
    cluster_name: Option<String>,
    seed_nodes: Vec<SocketAddr>,
    membership: Option<MembershipConfig>,
    consensus: Option<ConsensusConfig>,
    communication: Option<CommunicationConfig>,
    security: Option<SecurityConfig>,
    performance: Option<PerformanceConfig>,
    ha: Option<HaConfigCluster>,
}

impl ClusterConfigBuilder {
    /// Set the node ID
    #[must_use]
    pub fn node_id(mut self, node_id: NodeId) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// Set the bind address
    #[must_use]
    pub fn bind_address(mut self, address: SocketAddr) -> Self {
        self.bind_address = Some(address);
        self
    }

    /// Set the cluster name
    #[must_use]
    pub fn cluster_name<S: Into<String>>(mut self, name: S) -> Self {
        self.cluster_name = Some(name.into());
        self
    }

    /// Add a seed node
    #[must_use]
    pub fn add_seed_node(mut self, address: SocketAddr) -> Self {
        self.seed_nodes.push(address);
        self
    }

    /// Set seed nodes
    #[must_use]
    pub fn seed_nodes(mut self, nodes: Vec<SocketAddr>) -> Self {
        self.seed_nodes = nodes;
        self
    }

    /// Set membership configuration
    #[must_use]
    pub fn membership(mut self, config: MembershipConfig) -> Self {
        self.membership = Some(config);
        self
    }

    /// Set consensus configuration
    #[must_use]
    pub fn consensus(mut self, config: ConsensusConfig) -> Self {
        self.consensus = Some(config);
        self
    }

    /// Set communication configuration
    #[must_use]
    pub fn communication(mut self, config: CommunicationConfig) -> Self {
        self.communication = Some(config);
        self
    }

    /// Set security configuration
    #[must_use]
    pub fn security(mut self, config: SecurityConfig) -> Self {
        self.security = Some(config);
        self
    }

    /// Set performance configuration
    #[must_use]
    pub fn performance(mut self, config: PerformanceConfig) -> Self {
        self.performance = Some(config);
        self
    }

    /// Set HA configuration
    #[must_use]
    pub fn ha(mut self, config: HaConfigCluster) -> Self {
        self.ha = Some(config);
        self
    }

    /// Build the configuration
    ///
    /// # Errors
    /// Returns an error if required fields are missing or validation fails
    pub fn build(self) -> Result<ClusterConfig> {
        let config = ClusterConfig {
            node_id: self
                .node_id
                .ok_or_else(|| Error::Configuration("Node ID is required".to_string()))?,
            bind_address: self
                .bind_address
                .ok_or_else(|| Error::Configuration("Bind address is required".to_string()))?,
            cluster_name: self
                .cluster_name
                .ok_or_else(|| Error::Configuration("Cluster name is required".to_string()))?,
            seed_nodes: self.seed_nodes,
            membership: self.membership.unwrap_or_default(),
            consensus: self.consensus.unwrap_or_default(),
            communication: self.communication.unwrap_or_default(),
            security: self.security.unwrap_or_default(),
            performance: self.performance.unwrap_or_default(),
            ha: self.ha.unwrap_or_default(),
        };

        config.validate_config()?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_config_builder() -> Result<()> {
        let node_id = NodeId::generate();
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let config = ClusterConfig::builder()
            .node_id(node_id)
            .bind_address(bind_addr)
            .cluster_name("test-cluster")
            .add_seed_node("127.0.0.1:8081".parse().unwrap())
            .build()?;

        assert_eq!(config.node_id(), node_id);
        assert_eq!(config.bind_address(), bind_addr);
        assert_eq!(config.cluster_name(), "test-cluster");
        assert_eq!(config.seed_nodes.len(), 1);

        Ok(())
    }

    #[test]
    fn test_config_validation() {
        let node_id = NodeId::generate();
        let invalid_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);

        let result = ClusterConfig::builder()
            .node_id(node_id)
            .bind_address(invalid_addr)
            .cluster_name("test-cluster")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_default_configs() {
        let membership = MembershipConfig::default();
        assert_eq!(membership.protocol, MembershipProtocol::Swim);
        assert_eq!(membership.failure_timeout_ms, 5000);

        let consensus = ConsensusConfig::default();
        assert_eq!(consensus.algorithm, ConsensusAlgorithm::Raft);
        assert_eq!(consensus.election_timeout_ms, (1500, 3000));

        let communication = CommunicationConfig::default();
        assert_eq!(communication.max_connections, 10000);
        assert!(communication.enable_quic);

        let security = SecurityConfig::default();
        assert!(security.enable_tls);
        assert_eq!(security.min_tls_version, TlsVersion::V1_3);

        let performance = PerformanceConfig::default();
        assert_eq!(performance.allocator, Allocator::System);
        assert!(performance.numa_aware);
    }
}