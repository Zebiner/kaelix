//! High-performance message consumer for Kaelix.
//!
//! This module provides the message consumer functionality for the Kaelix streaming platform,
//! enabling applications to consume messages from topics with at-least-once delivery guarantees.
//!
//! ## Features
//!
//! - High-throughput message consumption
//! - Automatic consumer group management
//! - Offset management and tracking
//! - Graceful shutdown capabilities
//! - Built-in error handling and retries
//! - Consumer lag monitoring
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
//! let mut consumer = Consumer::new(config)?;
//!
//! consumer.subscribe(&["my-topic"])?;
//!
//! while let Some(message) = consumer.next().await {
//!     let message = message?;
//!     println!("Received: {:?}", message);
//!     consumer.commit()?;
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod config;
pub mod consumer;
pub mod offset;
pub mod subscription;

pub use config::ConsumerConfig;
pub use consumer::Consumer;
pub use offset::OffsetManager;
