//! # Kaelix Core
//!
//! Core streaming library for the `MemoryStreamer` ultra-high-performance distributed streaming system.
//!
//! This crate provides the fundamental building blocks for:
//! - Message definitions and serialization
//! - Binary protocol for wire communication
//! - Stream processing primitives
//! - Core data structures and algorithms
//! - Memory-efficient operations
//!
//! ## Performance Characteristics
//! - Target throughput: 10M+ messages/second
//! - Target latency: <10μs P99
//! - Protocol encode/decode: <1μs latency
//! - Zero-copy message handling where possible
//! - Lock-free data structures for critical paths
//!
//! ## Examples
//!
//! ```rust
//! use kaelix_core::Message;
//! use kaelix_core::protocol::{FrameType, Frame};
//!
//! // Create a message with zero-copy semantics
//! let payload = bytes::Bytes::from_static(b"hello world");
//! let message = Message::new("topic", payload);
//!
//! // Create a protocol frame
//! let frame = Frame::new(FrameType::Data, 1, bytes::Bytes::from_static(b"data"));
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

pub mod error;
pub mod message;
pub mod protocol;
pub mod stream;
pub mod types;

pub use error::{Error, Result};
pub use message::{Message, MessageId, Topic};
pub use types::{Offset, PartitionId, Timestamp};

/// Re-export commonly used types for convenience
pub mod prelude {
    pub use crate::protocol::{Frame, FrameDecoder, FrameEncoder, FrameFlags, FrameType};
    pub use crate::{Error, Message, MessageId, Result, Topic};
    pub use bytes::Bytes;
    pub use chrono::{DateTime, Utc};
    pub use uuid::Uuid;
}
