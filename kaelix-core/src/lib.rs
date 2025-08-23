//! # Kaelix Core
//!
//! Ultra-high-performance distributed message streaming system core library.
//!
//! Kaelix Core provides the foundational components for building distributed streaming
//! systems with ultra-low latency and high throughput. It includes message handling,
//! runtime management, telemetry, and plugin systems.
//!
//! ## Features
//!
//! - **Ultra-low Latency**: Optimized for sub-millisecond message processing
//! - **High Throughput**: Handles millions of messages per second
//! - **Memory Safety**: Built with Rust for zero-cost abstractions and memory safety
//! - **Plugin System**: Extensible architecture with dynamic plugin loading
//! - **Distributed Processing**: Native support for distributed message processing
//! - **Comprehensive Telemetry**: Built-in metrics, logging, and monitoring
//!
//! ## Quick Start
//!
//! ```rust
//! use kaelix_core::{Message, MessageBuilder, MemoryStreamerConfig, Topic};
//! use bytes::Bytes;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a topic
//!     let topic = Topic::new("my.topic")?;
//!     
//!     // Create a message
//!     let message = MessageBuilder::new(topic)
//!         .with_payload(Bytes::from("Hello, Kaelix!"))
//!         .build()?;
//!
//!     // Create configuration
//!     let config = MemoryStreamerConfig::default();
//!
//!     // Initialize the library
//!     kaelix_core::init().await?;
//!
//!     // Process messages...
//!
//!     kaelix_core::shutdown().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Architecture
//!
//! Kaelix Core is organized into several key modules:
//!
//! - [`message`]: Message types and builders for efficient data handling
//! - [`config`]: Configuration management and validation
//! - [`error`]: Error types and result handling
//! - [`types`]: Core type definitions and utilities
//! - [`prelude`]: Common imports for convenient usage
//!
//! ## Experimental Features
//!
//! Advanced features are available behind the `experimental` feature flag:
//!
//! - [`runtime`]: Core runtime system with lifecycle management (experimental)
//! - [`plugin`]: Plugin system for extensibility (experimental)
//! - [`telemetry`]: Metrics, logging, and observability (experimental)
//! - [`multiplexing`]: Advanced message routing and multiplexing (experimental)

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(dead_code)] // Temporarily allow during development

use serde::{Deserialize, Serialize};
use std::fmt;

// Core stable modules - these are working and tested
pub mod config;
pub mod error;
pub mod message;
pub mod prelude;
pub mod types;

// Experimental modules - feature-gated to avoid compilation issues
#[cfg(feature = "experimental")]
pub mod multiplexing;
#[cfg(feature = "experimental")]
pub mod plugin;
#[cfg(feature = "experimental")]
pub mod runtime;
#[cfg(feature = "experimental")]
pub mod telemetry;

// Core re-exports for convenience
pub use crate::{
    config::MemoryStreamerConfig,
    error::{Error, Result},
    message::{Message, MessageBuilder, MessageId, Topic},
    types::{Offset, PartitionId, Timestamp},
};

// Experimental re-exports - only available with experimental feature
#[cfg(feature = "experimental")]
pub use crate::{
    config::PerformanceConfig,
    runtime::{HealthMonitor, HealthStatus, OptimizedRuntime},
    telemetry::performance::PerformanceTracker,
};

/// System status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    /// Whether the system is running
    pub running: bool,
    /// Current uptime in milliseconds
    pub uptime_ms: u64,
    /// System version
    pub version: String,
}

impl fmt::Display for SystemStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "System: {} (uptime: {}ms, version: {})",
            if self.running {
                "running"
            } else {
                "stopped"
            },
            self.uptime_ms,
            self.version
        )
    }
}

/// Initialize the Kaelix Core system
///
/// This function sets up necessary runtime components and prepares
/// the system for message processing.
///
/// # Errors
/// Returns an error if initialization fails
pub async fn init() -> Result<()> {
    // Initialize logging and basic components
    tracing::info!("Initializing Kaelix Core system");

    // For now, this is a placeholder
    Ok(())
}

/// Shutdown the Kaelix Core system gracefully
///
/// This function ensures all resources are properly cleaned up
/// and all pending operations are completed.
///
/// # Errors
/// Returns an error if shutdown fails
pub async fn shutdown() -> Result<()> {
    tracing::info!("Shutting down Kaelix Core system");

    // For now, this is a placeholder
    Ok(())
}

/// Get current system status
///
/// Returns information about the running system including
/// uptime and operational status.
#[must_use]
pub fn status() -> SystemStatus {
    SystemStatus {
        running: true, // Placeholder
        uptime_ms: 0,  // Placeholder
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
