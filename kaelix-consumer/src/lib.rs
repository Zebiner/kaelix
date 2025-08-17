//! # Kaelix Consumer
//!
//! High-performance message consumer client for the MemoryStreamer distributed streaming system.
//!
//! This crate provides:
//! - High-throughput message consumption
//! - Automatic offset management and tracking
//! - Consumer group coordination
//! - Async stream processing capabilities
//! - Configurable backpressure and flow control
//!
//! ## Performance Features
//! - Zero-copy message deserialization
//! - Batched message fetching
//! - Parallel message processing
//! - Automatic partition balancing
//!
//! ## Examples
//!
//! ```rust
//! use kaelix_consumer::{Consumer, ConsumerConfig};
//! use futures::StreamExt;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = ConsumerConfig::default();
//! let mut consumer = Consumer::new(config).await?;
//!
//! consumer.subscribe(&["my-topic"]).await?;
//!
//! while let Some(message) = consumer.next().await {
//!     let message = message?;
//!     println!("Received: {:?}", message);
//!     consumer.commit().await?;
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

pub mod consumer;
pub mod config;
pub mod subscription;
pub mod offset;

pub use consumer::{Consumer, ConsumedMessage};
pub use config::ConsumerConfig;
pub use kaelix_core::{Error, Result};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{Consumer, ConsumerConfig, ConsumedMessage};
    pub use kaelix_core::prelude::*;
}