//! # Kaelix Broker
//!
//! High-performance message broker implementation for the MemoryStreamer distributed streaming system.
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
//! use kaelix_broker::{Broker, BrokerConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = BrokerConfig::default();
//! let broker = Broker::new(config).await?;
//! broker.start().await?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

pub mod broker;
pub mod config;
pub mod storage;
pub mod network;
pub mod routing;

pub use broker::{Broker, BrokerHandle};
pub use config::BrokerConfig;
pub use kaelix_core::{Error, Result};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{Broker, BrokerConfig, BrokerHandle};
    pub use kaelix_core::prelude::*;
}