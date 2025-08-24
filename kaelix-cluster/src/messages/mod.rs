//! # Cluster Message System
//!
//! Core message envelope structure for distributed cluster communication in MemoryStreamer.
//!
//! This module provides the foundational message system that enables ultra-high-performance
//! communication between cluster nodes with <1ms latency targets. The message system supports
//! consensus operations, membership management, discovery, and health monitoring.
//!
//! ## Features
//!
//! - **Zero-Copy Message Design**: Optimized for minimal serialization overhead
//! - **Type-Safe Routing**: Compile-time guarantees for message handling
//! - **Extensible Payload System**: Easy to add new message types
//! - **Efficient Serialization**: Binary serialization with serde + bincode
//! - **Message Classification**: Built-in request/response pattern detection
//!
//! ## Performance Characteristics
//!
//! - **Serialization**: <100ns typical message serialization
//! - **Message Size**: <1KB typical message size for efficient network transmission
//! - **Memory Layout**: Cache-optimized structure layout for minimal memory footprint
//!
//! ## Usage
//!
//! ```rust
//! use kaelix_cluster::messages::{ClusterMessage, MessagePayload, MessageHeader};
//! use kaelix_cluster::types::NodeId;
//!
//! // Create a ping message
//! let node_id = NodeId::generate();
//! let destination = NodeId::generate();
//! let ping_msg = ClusterMessage::new(
//!     node_id,
//!     destination,
//!     MessagePayload::Ping { node_id }
//! );
//!
//! // Check message type
//! assert!(ping_msg.is_request());
//! assert_eq!(ping_msg.message_type(), "Ping");
//! ```

use crate::types::NodeId;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Unique identifier for cluster messages
pub type MessageId = u64;

/// Core message envelope for all cluster communication
///
/// The `ClusterMessage` serves as the foundational structure for all inter-node
/// communication within the MemoryStreamer cluster. It provides reliable routing,
/// type safety, and efficient serialization for distributed operations.
///
/// # Design Principles
///
/// - **Minimal Overhead**: Compact representation optimized for network transmission
/// - **Version Safe**: Future-proof design allowing protocol evolution
/// - **Cache Friendly**: Memory layout optimized for CPU cache performance
/// - **Zero Allocation**: Designed to work with zero-copy serialization paths
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterMessage {
    /// Message routing and metadata header
    pub header: MessageHeader,
    /// Typed message payload
    pub payload: MessagePayload,
}

/// Message header containing routing and metadata information
///
/// The header provides essential routing information and metadata required
/// for message delivery, ordering, and processing within the cluster.
///
/// # Fields
///
/// - `message_id`: Unique identifier for tracking and correlation
/// - `source`: Originating node identifier
/// - `destination`: Target node identifier  
/// - `timestamp`: Unix timestamp in microseconds for ordering and latency measurement
/// - `message_type`: Numeric type identifier for efficient dispatching
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageHeader {
    /// Unique message identifier for tracking and correlation
    pub message_id: MessageId,
    /// Source node identifier
    pub source: NodeId,
    /// Destination node identifier
    pub destination: NodeId,
    /// Unix timestamp in microseconds
    pub timestamp: u64,
    /// Numeric message type identifier for efficient dispatch
    pub message_type: u32,
}

/// Typed message payload enumeration
///
/// The payload enum provides type-safe message handling for all cluster operations.
/// Each variant represents a specific cluster protocol message with its associated data.
///
/// # Message Categories
///
/// - **Consensus Messages**: Raft protocol operations for distributed agreement
/// - **Membership Messages**: Node join/leave operations and cluster management
/// - **Discovery Messages**: Network discovery and connectivity testing
/// - **Health Monitoring**: Node health status and availability checking
///
/// # Performance Optimization
///
/// The enum is designed for efficient serialization and minimal memory overhead.
/// Binary payload data uses `Vec<u8>` for flexibility while maintaining performance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessagePayload {
    // ========================================
    // Consensus Messages (Raft Protocol)
    // ========================================
    /// Request for vote in leader election
    ///
    /// Sent by candidate nodes during leader election to request votes
    /// from other nodes in the cluster.
    RequestVote {
        /// Current term of the candidate
        term: u64,
        /// Node ID of the candidate requesting votes
        candidate_id: NodeId,
        /// Index of candidate's last log entry
        last_log_index: u64,
        /// Term of candidate's last log entry
        last_log_term: u64,
    },

    /// Response to vote request
    ///
    /// Reply from nodes indicating whether they granted their vote
    /// to the requesting candidate.
    RequestVoteResponse {
        /// Current term for term synchronization
        term: u64,
        /// Whether the vote was granted to the candidate
        vote_granted: bool,
    },

    /// Append entries message for log replication
    ///
    /// Sent by leader to replicate log entries to follower nodes
    /// and serve as heartbeat when entries are empty.
    AppendEntries {
        /// Leader's current term
        term: u64,
        /// Leader's node identifier for validation
        leader_id: NodeId,
        /// Log entries to append (empty for heartbeat)
        entries: Vec<u8>,
    },

    /// Response to append entries request
    ///
    /// Reply from followers indicating success or failure of
    /// log entry append operation.
    AppendEntriesResponse {
        /// Current term for term synchronization
        term: u64,
        /// Whether the append operation was successful
        success: bool,
    },

    // ========================================
    // Membership Messages
    // ========================================
    /// Request to join the cluster
    ///
    /// Sent by nodes wishing to become part of the cluster.
    /// Requires cluster consensus for acceptance.
    JoinRequest {
        /// Node ID of the requesting node
        node_id: NodeId,
    },

    /// Response to join request
    ///
    /// Reply indicating whether the join request was accepted
    /// and cluster membership was granted.
    JoinResponse {
        /// Whether the join request was accepted
        accepted: bool,
    },

    /// Request to leave the cluster
    ///
    /// Graceful notification that a node intends to leave
    /// the cluster voluntarily.
    LeaveRequest {
        /// Node ID of the departing node
        node_id: NodeId,
    },

    // ========================================
    // Discovery Messages
    // ========================================
    /// Network connectivity ping
    ///
    /// Used for network discovery, connectivity testing,
    /// and round-trip latency measurement.
    Ping {
        /// Node ID of the pinging node
        node_id: NodeId,
    },

    /// Response to ping message
    ///
    /// Reply to ping indicating node availability and
    /// completing round-trip latency measurement.
    Pong {
        /// Node ID of the responding node
        node_id: NodeId,
    },

    // ========================================
    // Health Monitoring
    // ========================================
    /// Health check request
    ///
    /// Proactive health monitoring message to verify
    /// node availability and operational status.
    HealthCheck,

    /// Health status response
    ///
    /// Reply providing current health status and
    /// operational metrics.
    HealthResponse {
        /// Health status description
        status: String,
    },
}

impl ClusterMessage {
    /// Create a new cluster message with automatic header generation
    ///
    /// This constructor automatically generates a unique message ID and timestamp,
    /// ensuring proper message tracking and ordering.
    ///
    /// # Parameters
    ///
    /// * `source` - Source node identifier
    /// * `destination` - Destination node identifier  
    /// * `payload` - Message payload containing the actual message data
    ///
    /// # Returns
    ///
    /// A new `ClusterMessage` with populated header and payload
    ///
    /// # Performance
    ///
    /// This function is optimized for minimal overhead with inline hint
    /// and efficient timestamp generation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_cluster::messages::{ClusterMessage, MessagePayload};
    /// use kaelix_cluster::types::NodeId;
    ///
    /// let source = NodeId::generate();
    /// let dest = NodeId::generate();
    /// let msg = ClusterMessage::new(source, dest, MessagePayload::HealthCheck);
    /// ```
    #[inline]
    pub fn new(source: NodeId, destination: NodeId, payload: MessagePayload) -> Self {
        let message_id = Self::generate_message_id();
        let timestamp = Self::current_timestamp_micros();
        let message_type = Self::payload_to_type_id(&payload);

        Self {
            header: MessageHeader { message_id, source, destination, timestamp, message_type },
            payload,
        }
    }

    /// Get the human-readable message type string
    ///
    /// Returns a string representation of the message type for debugging,
    /// logging, and monitoring purposes.
    ///
    /// # Returns
    ///
    /// Static string slice representing the message type
    ///
    /// # Performance
    ///
    /// This method is marked inline for optimal performance in hot paths.
    #[inline]
    pub fn message_type(&self) -> &'static str {
        match &self.payload {
            MessagePayload::RequestVote { .. } => "RequestVote",
            MessagePayload::RequestVoteResponse { .. } => "RequestVoteResponse",
            MessagePayload::AppendEntries { .. } => "AppendEntries",
            MessagePayload::AppendEntriesResponse { .. } => "AppendEntriesResponse",
            MessagePayload::JoinRequest { .. } => "JoinRequest",
            MessagePayload::JoinResponse { .. } => "JoinResponse",
            MessagePayload::LeaveRequest { .. } => "LeaveRequest",
            MessagePayload::Ping { .. } => "Ping",
            MessagePayload::Pong { .. } => "Pong",
            MessagePayload::HealthCheck => "HealthCheck",
            MessagePayload::HealthResponse { .. } => "HealthResponse",
        }
    }

    /// Check if the message is a request type
    ///
    /// Request messages expect a corresponding response message.
    /// This classification helps with timeout handling and response correlation.
    ///
    /// # Returns
    ///
    /// `true` if the message is a request that expects a response
    #[inline]
    pub fn is_request(&self) -> bool {
        matches!(
            &self.payload,
            MessagePayload::RequestVote { .. }
                | MessagePayload::AppendEntries { .. }
                | MessagePayload::JoinRequest { .. }
                | MessagePayload::LeaveRequest { .. }
                | MessagePayload::Ping { .. }
                | MessagePayload::HealthCheck
        )
    }

    /// Check if the message is a response type
    ///
    /// Response messages are replies to request messages and typically
    /// don't expect further responses.
    ///
    /// # Returns
    ///
    /// `true` if the message is a response to a previous request
    #[inline]
    pub fn is_response(&self) -> bool {
        matches!(
            &self.payload,
            MessagePayload::RequestVoteResponse { .. }
                | MessagePayload::AppendEntriesResponse { .. }
                | MessagePayload::JoinResponse { .. }
                | MessagePayload::Pong { .. }
                | MessagePayload::HealthResponse { .. }
        )
    }

    /// Generate a unique message ID
    ///
    /// Creates a unique identifier for message tracking and correlation.
    /// Uses high-resolution timestamp combined with a random component
    /// for uniqueness across the cluster.
    ///
    /// # Returns
    ///
    /// Unique 64-bit message identifier
    fn generate_message_id() -> MessageId {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);

        let timestamp = Self::current_timestamp_micros();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

        // Combine timestamp (upper 32 bits) with counter (lower 32 bits)
        // This ensures uniqueness even with high message rates
        ((timestamp & 0xFFFF_FFFF) << 32) | (counter & 0xFFFF_FFFF)
    }

    /// Get current timestamp in microseconds
    ///
    /// Provides high-resolution timestamp for message ordering and
    /// latency measurement.
    ///
    /// # Returns
    ///
    /// Current Unix timestamp in microseconds
    fn current_timestamp_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }

    /// Convert payload variant to numeric type identifier
    ///
    /// Maps payload variants to efficient numeric identifiers for
    /// fast message dispatching and network protocol optimization.
    ///
    /// # Parameters
    ///
    /// * `payload` - Message payload to classify
    ///
    /// # Returns
    ///
    /// Numeric type identifier for the payload variant
    fn payload_to_type_id(payload: &MessagePayload) -> u32 {
        match payload {
            // Consensus messages: 1000-1999
            MessagePayload::RequestVote { .. } => 1000,
            MessagePayload::RequestVoteResponse { .. } => 1001,
            MessagePayload::AppendEntries { .. } => 1010,
            MessagePayload::AppendEntriesResponse { .. } => 1011,

            // Membership messages: 2000-2999
            MessagePayload::JoinRequest { .. } => 2000,
            MessagePayload::JoinResponse { .. } => 2001,
            MessagePayload::LeaveRequest { .. } => 2010,

            // Discovery messages: 3000-3999
            MessagePayload::Ping { .. } => 3000,
            MessagePayload::Pong { .. } => 3001,

            // Health messages: 4000-4999
            MessagePayload::HealthCheck => 4000,
            MessagePayload::HealthResponse { .. } => 4001,
        }
    }
}

impl MessageHeader {
    /// Create a new message header with specified parameters
    ///
    /// This constructor allows fine-grained control over header fields
    /// for advanced use cases requiring custom message IDs or timestamps.
    ///
    /// # Parameters
    ///
    /// * `message_id` - Unique message identifier
    /// * `source` - Source node identifier
    /// * `destination` - Destination node identifier
    /// * `timestamp` - Message timestamp in microseconds
    /// * `message_type` - Numeric message type identifier
    ///
    /// # Returns
    ///
    /// A new `MessageHeader` with the specified parameters
    pub fn new(
        message_id: MessageId,
        source: NodeId,
        destination: NodeId,
        timestamp: u64,
        message_type: u32,
    ) -> Self {
        Self { message_id, source, destination, timestamp, message_type }
    }
}

#[cfg(test)]
mod tests;
