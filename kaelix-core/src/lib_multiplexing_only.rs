//! Kaelix Core Library - Multiplexing Only
//!
//! Temporary lib.rs for testing multiplexing module compilation only

pub mod config;
pub mod error;
pub mod message;
pub mod multiplexing;
pub mod protocol;
pub mod runtime;
pub mod stream;
pub mod types;

// Core exports
pub use error::{Error, Result};
pub use message::{Message, MessageBuilder, MessageId};
pub use types::{Offset, PartitionId, Timestamp};

// Stream multiplexing exports
pub use multiplexing::{
    StreamMultiplexer, StreamId, StreamConfig, StreamMetrics, StreamPriority,
    BackpressureLevel, Priority, RoutingType, MultiplexerConfig, HealthStatus,
};

// Re-export commonly used types for convenience
pub mod prelude {
    pub use crate::config::{ConfigLoader, MemoryStreamerConfig};
    pub use crate::multiplexing::{
        StreamMultiplexer, StreamConfig, StreamPriority, StreamState,
        BackpressureLevel, RoutingType, MultiplexerConfig, HealthStatus,
        StreamId, GroupId, StreamMetrics,
    };
    pub use crate::protocol::{Frame, FrameDecoder, FrameEncoder, FrameFlags, FrameType};
    pub use crate::runtime::{OptimizedRuntime, RuntimeConfig};
    pub use crate::{Error, Message, MessageId, Result};
    pub use bytes::Bytes;
    pub use chrono::{DateTime, Utc};
    pub use uuid::Uuid;
}

/// Performance constraints and targets for MemoryStreamer
pub mod performance {
    /// Target throughput: 10M+ messages per second
    pub const TARGET_THROUGHPUT_MPS: u64 = 10_000_000;
    
    /// Target P99 latency: <10 microseconds
    pub const TARGET_P99_LATENCY_US: u64 = 10;
    
    /// Stream multiplexing performance targets
    pub mod multiplexing {
        /// Target concurrent streams: 1M+
        pub const TARGET_CONCURRENT_STREAMS: usize = 1_000_000;
        
        /// Target stream lookup latency: <10ns
        pub const TARGET_STREAM_LOOKUP_NS: u64 = 10;
        
        /// Target routing decision time: <100ns
        pub const TARGET_ROUTING_DECISION_NS: u64 = 100;
        
        /// Target memory per inactive stream: <1KB
        pub const TARGET_MEMORY_PER_INACTIVE_STREAM_BYTES: usize = 1024;
        
        /// Target scheduling decision time: <50ns
        pub const TARGET_SCHEDULING_DECISION_NS: u64 = 50;
        
        /// Target backpressure propagation time: <1μs
        pub const TARGET_BACKPRESSURE_PROPAGATION_US: u64 = 1;
    }
}

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");