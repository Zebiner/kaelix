//! # Kaelix Broker
//!
//! High-performance message broker implementation for the `MemoryStreamer` distributed streaming system.
//!
//! This crate provides:
//! - Message routing and delivery
//! - Topic management and partitioning
//! - Client connection handling
//! - Storage and persistence layer
//! - Replication and clustering
//!
//! ## Performance Targets
//! - 10M+ messages/second throughput
//! - <10μs P99 latency
//! - Horizontal scalability
//! - Zero-downtime operations
//!
//! ## Examples
//!
//! ```rust
//! use kaelix_broker::{MessageBroker, BrokerConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = BrokerConfig::default();
//! let broker = MessageBroker::new();
//! broker.start()?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

pub mod broker;
pub mod config;
pub mod network;
pub mod routing;
pub mod storage;

pub use broker::{BrokerState, MessageBroker};
pub use config::BrokerConfig;
pub use kaelix_core::{Error, Result};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{BrokerConfig, BrokerState, MessageBroker};
    pub use kaelix_core::prelude::*;
}
