//! # Kaelix Cluster
//!
//! Ultra-high-performance distributed clustering and consensus library for MemoryStreamer.
//!
//! Kaelix Cluster provides the foundational components for building distributed streaming
//! systems with ultra-low latency consensus, membership management, and inter-node communication.
//! Designed for <1ms consensus operations and supporting 1M+ concurrent connections.
//!
//! ## Features
//!
//! - **Ultra-low Latency Consensus**: <1ms consensus operations with optimized Raft implementation
//! - **High-Performance Membership**: SWIM-based membership protocol with failure detection
//! - **Zero-Copy Communication**: Lock-free inter-node message passing
//! - **NUMA-Aware Design**: Optimized for modern multi-socket architectures  
//! - **Memory Efficient**: <1KB per inactive stream, optimized memory layout
//! - **Security First**: End-to-end encryption with hardware acceleration
//!
//! ## Quick Start
//!
//! ```rust
//! use kaelix_cluster::{ClusterConfig, Node, ClusterNodeId};
//! use std::net::SocketAddr;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create cluster configuration
//!     let config = ClusterConfig::builder()
//!         .node_id(ClusterNodeId::generate())
//!         .bind_address("127.0.0.1:8000".parse::<SocketAddr>()?)
//!         .cluster_name("my-cluster")
//!         .build()?;
//!     
//!     // Initialize node
//!     let node = Node::new(config).await?;
//!     
//!     // Join cluster
//!     node.join_cluster().await?;
//!     
//!     // Node is now part of the cluster...
//!     
//!     node.shutdown().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Architecture
//!
//! Kaelix Cluster is organized into several key modules:
//!
//! - [`node`]: Node identification, addressing, and metadata management
//! - [`membership`]: Node discovery, failure detection, and cluster membership management
//! - [`consensus`]: Distributed consensus algorithms (Raft, PBFT) for state synchronization
//! - [`communication`]: High-performance inter-node communication with zero-copy design
//! - [`messages`]: Core message envelope system for all cluster communication
//! - [`config`]: Configuration management and validation for cluster operations
//! - [`time`]: Distributed timestamp and versioning types for event ordering and causality
//! - [`error`]: Comprehensive error handling for distributed systems scenarios
//!
//! ## Performance Targets
//!
//! - **Consensus Latency**: <1ms P99 for typical operations
//! - **Membership Updates**: <100μs propagation across 1000+ nodes
//! - **Network Throughput**: 10M+ messages/second per node
//! - **Connection Capacity**: 1M+ concurrent inter-node connections
//! - **Memory Efficiency**: <1KB per inactive connection
//!
//! ## Consensus Algorithms
//!
//! Multiple consensus algorithms are supported based on use case requirements:
//!
//! - **Fast Raft**: Optimized Raft implementation for low-latency scenarios
//! - **Pipeline Raft**: High-throughput variant with request pipelining
//! - **Multi-Raft**: Parallel consensus for independent data partitions
//!
//! ## Membership Protocols
//!
//! - **SWIM**: Scalable weakly-consistent infection-style membership protocol
//! - **Rapid**: Fast failure detection with configurable accuracy guarantees
//! - **Hybrid**: Combined approach for different cluster scales

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(dead_code)] // Temporarily allow during development

use serde::{Deserialize, Serialize};
use std::fmt;

// Core modules
pub mod communication;
pub mod config;
pub mod consensus;
pub mod error;
pub mod membership;
pub mod messages;
pub mod node;
pub mod time;

// Testing infrastructure (conditionally compiled)
#[cfg(any(test, feature = "dev"))]
pub mod test_utils;

// Re-exports for convenience
pub use crate::{
    communication::{MessageRouter, NetworkTransport},
    config::{ClusterConfig, ClusterConfigBuilder},
    consensus::ConsensusState,
    error::{Error, Result},
    membership::{NodeInfo, NodeStatus},
    messages::{ClusterMessage, MessageHeader, MessageId, MessagePayload},
    node::{NodeId, NodeAddress, NodeMetadata, NodeRole, NodeCapabilities, NodeStatus as NodeOperationalStatus, NodeSelector, Protocol},
    time::{LogicalClock, VectorClock, VersionVector, HybridLogicalClock, ClockSynchronizer, TimestampUtils, CausalityRelation, compare_causality},
};

/// Current types module for internal use
pub mod types {
    use serde::{Deserialize, Serialize};
    use std::fmt;
    use uuid::Uuid;

    /// Node identifier type
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct NodeId(Uuid);

    impl NodeId {
        /// Generate a new unique node ID
        #[must_use]
        pub fn generate() -> Self {
            Self(Uuid::new_v4())
        }

        /// Create a node ID from a UUID
        #[must_use]
        pub const fn from_uuid(uuid: Uuid) -> Self {
            Self(uuid)
        }

        /// Get the underlying UUID
        #[must_use]
        pub const fn as_uuid(&self) -> &Uuid {
            &self.0
        }
    }

    impl fmt::Display for NodeId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Default for NodeId {
        fn default() -> Self {
            Self::generate()
        }
    }

    /// Term represents a logical timestamp in consensus algorithms
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    pub struct Term(u64);

    impl Term {
        /// Create a new term
        pub const fn new(value: u64) -> Self {
            Self(value)
        }

        /// Get the term value
        pub const fn value(&self) -> u64 {
            self.0
        }

        /// Increment the term
        pub fn increment(&mut self) {
            self.0 += 1;
        }
    }

    impl Default for Term {
        fn default() -> Self {
            Self::new(0)
        }
    }

    impl fmt::Display for Term {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }
}

/// Cluster status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterStatus {
    /// Node is initializing
    Initializing,
    /// Node is active in the cluster
    Active,
    /// Node is joining the cluster
    Joining,
    /// Node is leaving the cluster
    Leaving,
    /// Node has left the cluster
    Left,
    /// Node is in failed state
    Failed,
}

impl fmt::Display for ClusterStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = match self {
            Self::Initializing => "initializing",
            Self::Active => "active",
            Self::Joining => "joining",
            Self::Leaving => "leaving",
            Self::Left => "left",
            Self::Failed => "failed",
        };
        write!(f, "{status}")
    }
}

impl Default for ClusterStatus {
    fn default() -> Self {
        Self::Initializing
    }
}

/// Cluster node unique identifier (alias for types::NodeId)
pub type ClusterNodeId = types::NodeId;

/// Cluster node implementation
#[derive(Debug)]
pub struct Node {
    /// Node configuration
    config: ClusterConfig,
    /// Current cluster status
    status: ClusterStatus,
}

impl Node {
    /// Create a new cluster node
    /// # Errors
    /// Returns an error if node initialization fails
    pub async fn new(config: ClusterConfig) -> Result<Self> {
        tracing::info!("Creating new cluster node with ID: {}", config.node_id());

        // Validate configuration before creating node
        config.validate_config()?;

        Ok(Self { config, status: ClusterStatus::Initializing })
    }

    /// Get the node's current cluster status
    #[must_use]
    pub const fn status(&self) -> ClusterStatus {
        self.status
    }

    /// Get the node configuration
    #[must_use]
    pub const fn config(&self) -> &ClusterConfig {
        &self.config
    }

    /// Get the node ID
    #[must_use]
    pub fn node_id(&self) -> ClusterNodeId {
        self.config.node_id()
    }

    /// Join the cluster
    /// # Errors
    /// Returns an error if joining the cluster fails
    pub async fn join_cluster(&mut self) -> Result<()> {
        tracing::info!("Node {} joining cluster '{}'", self.node_id(), self.config.cluster_name());

        self.status = ClusterStatus::Joining;

        // Placeholder for actual cluster join logic
        // This would involve:
        // 1. Connecting to seed nodes
        // 2. Performing membership protocol handshake
        // 3. Synchronizing initial state
        // 4. Starting consensus participation

        self.status = ClusterStatus::Active;
        tracing::info!("Node {} successfully joined cluster", self.node_id());

        Ok(())
    }

    /// Leave the cluster gracefully
    /// # Errors
    /// Returns an error if leaving the cluster fails
    pub async fn leave_cluster(&mut self) -> Result<()> {
        tracing::info!("Node {} leaving cluster", self.node_id());

        self.status = ClusterStatus::Leaving;

        // Placeholder for graceful cluster leave logic
        // This would involve:
        // 1. Announcing intention to leave
        // 2. Transferring leadership if leader
        // 3. Waiting for state synchronization
        // 4. Closing connections gracefully

        self.status = ClusterStatus::Left;
        tracing::info!("Node {} successfully left cluster", self.node_id());

        Ok(())
    }

    /// Shutdown the node
    /// # Errors
    /// Returns an error if shutdown fails
    pub async fn shutdown(mut self) -> Result<()> {
        tracing::info!("Shutting down node {}", self.node_id());

        // Leave cluster if still active
        if matches!(self.status, ClusterStatus::Active | ClusterStatus::Joining) {
            self.leave_cluster().await?;
        }

        // Cleanup resources
        // Placeholder for resource cleanup

        tracing::info!("Node shutdown completed");
        Ok(())
    }
}

/// Initialize the Kaelix Cluster system
///
/// This function sets up necessary cluster components and prepares
/// the system for distributed operations.
///
/// # Errors
/// Returns an error if initialization fails
pub async fn init() -> Result<()> {
    tracing::info!("Initializing Kaelix Cluster system");

    // Initialize cluster-wide components
    // Placeholder for actual initialization

    Ok(())
}

/// Shutdown the Kaelix Cluster system gracefully
///
/// This function ensures all cluster resources are properly cleaned up
/// and all nodes are gracefully disconnected.
///
/// # Errors
/// Returns an error if shutdown fails
pub async fn shutdown() -> Result<()> {
    tracing::info!("Shutting down Kaelix Cluster system");

    // Graceful cluster shutdown
    // Placeholder for actual shutdown logic

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn test_cluster_node_id_generation() {
        let id1 = ClusterNodeId::generate();
        let id2 = ClusterNodeId::generate();

        assert_ne!(id1, id2);
        assert_ne!(id1.to_string(), id2.to_string());
    }

    #[test]
    fn test_cluster_status_display() {
        assert_eq!(ClusterStatus::Initializing.to_string(), "initializing");
        assert_eq!(ClusterStatus::Active.to_string(), "active");
        assert_eq!(ClusterStatus::Left.to_string(), "left");
    }

    #[tokio::test]
    async fn test_node_creation() -> Result<()> {
        let config = ClusterConfig::builder()
            .node_id(ClusterNodeId::generate())
            .bind_address("127.0.0.1:9999".parse::<SocketAddr>().unwrap()) // Use a proper test port
            .cluster_name("test-cluster")
            .build()?;

        let node = Node::new(config).await?;
        assert_eq!(node.status(), ClusterStatus::Initializing);

        Ok(())
    }

    #[test]
    fn test_new_node_types() {
        // Test new NodeId
        let node_id = NodeId::generate();
        assert!(node_id.is_valid());

        // Test NodeAddress
        let address = NodeAddress::tcp("127.0.0.1".parse().unwrap(), 8080);
        assert!(address.is_valid());

        // Test NodeMetadata
        let metadata = NodeMetadata::new(node_id, address)
            .with_role(NodeRole::Follower)
            .with_region("us-west-2");
        assert!(metadata.is_healthy());

        // Test NodeSelector
        let selector = NodeSelector::new()
            .with_role(NodeRole::Leader)
            .with_min_health_score(0.8);
        assert!(!selector.is_empty());
    }

    #[test]
    fn test_time_module_integration() {
        // Test LogicalClock
        let mut logical_clock = LogicalClock::new();
        let timestamp = logical_clock.tick();
        assert_eq!(timestamp, 1);

        // Test VectorClock
        let node_id = NodeId::generate();
        let mut vector_clock = VectorClock::new(node_id);
        let vector_time = vector_clock.tick();
        assert_eq!(vector_time, 1);

        // Test HybridLogicalClock
        let hlc = HybridLogicalClock::now();
        assert!(hlc.physical_time() > 0);

        // Test TimestampUtils
        let current_time = TimestampUtils::system_time_millis();
        assert!(current_time > 0);
    }
}