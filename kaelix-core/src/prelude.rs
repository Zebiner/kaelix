//! # Prelude
//!
//! The prelude module provides convenient access to commonly used types and traits
//! from the Kaelix Core library.

pub use crate::{
    config::{MemoryStreamerConfig, ValidationContext},
    error::{Error, Result},
    message::{Message, MessageBuilder, MessageId, Topic},
    types::{Offset, PartitionId, Timestamp},
};

// Re-export commonly used traits
pub use bytes::Bytes;
pub use chrono::{DateTime, Utc};
pub use serde::{Deserialize, Serialize};
pub use uuid::Uuid;

// Experimental features
#[cfg(feature = "experimental")]
pub use crate::{
    config::PerformanceConfig,
    multiplexing::{Multiplexer, MultiplexingConfig},
    plugin::{Plugin, PluginManager},
    runtime::{HealthMonitor, HealthStatus, OptimizedRuntime},
    telemetry::{
        metrics::{MetricsCollector, MetricsRegistry},
        tracing::{TracingConfig, TracingLevel},
    },
};
