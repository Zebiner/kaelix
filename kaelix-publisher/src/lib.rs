//! High-performance message publisher for Kaelix.
//!
//! This module provides the message publishing functionality for the Kaelix streaming platform,
//! enabling applications to publish messages to topics with at-least-once delivery guarantees.
//!
//! ## Features
//!
//! - High-throughput message publishing
//! - Automatic batching for efficiency
//! - Configurable retry policies
//! - Compression support
//! - Message ordering guarantees
//! - Back-pressure handling
//! - Producer acknowledgments
//! - Asynchronous and synchronous APIs
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
//! let publisher = Publisher::new(config)?;
//!
//! let message = Message::new("my-topic", Bytes::from("hello world"))?;
//! publisher.publish(&message)?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod batch;
pub mod config;
pub mod connection;
pub mod publisher;

pub use config::PublisherConfig;
pub use publisher::Publisher;
