//! # Kaelix Publisher
//!
//! High-performance message publisher client for the MemoryStreamer distributed streaming system.
//!
//! This crate provides:
//! - High-throughput message publishing
//! - Batched message sending for optimal performance
//! - Connection pooling and management
//! - Async/await support with backpressure handling
//! - Error handling and retry mechanisms
//!
//! ## Performance Features
//! - Zero-copy message serialization
//! - Connection pooling for scalability
//! - Batched publishing for reduced overhead
//! - Configurable backpressure handling
//!
//! ## Examples
//!
//! ```rust
//! use kaelix_publisher::{Publisher, PublisherConfig};
//! use kaelix_core::Message;
//! use bytes::Bytes;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PublisherConfig::default();
//! let publisher = Publisher::new(config).await?;
//!
//! let message = Message::new("my-topic", Bytes::from("hello world"))?;
//! publisher.publish(message).await?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

pub mod publisher;
pub mod config;
pub mod connection;
pub mod batch;

pub use publisher::{Publisher, PublishResult};
pub use config::PublisherConfig;
pub use kaelix_core::{Error, Result};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{Publisher, PublisherConfig, PublishResult};
    pub use kaelix_core::prelude::*;
}