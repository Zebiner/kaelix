//! # Configuration Management
//!
//! Provides comprehensive configuration loading, validation, and hot-reloading
//! capabilities for the Kaelix Core library.

pub mod hot_reload;
pub mod loader;
/// Configuration schema definitions and validation types
pub mod schema;
pub mod validator;

// Re-export main types for convenience
pub use hot_reload::{ConfigWatcher, HotReloadManager};
pub use loader::ConfigLoader;
pub use schema::{
    KaelixConfig, MemoryStreamerConfig, NetworkConfig, PerformanceConfig, ProtocolConfig,
    SecurityConfig, StorageConfig, TelemetryConfig, TracingLevel, ValidationContext,
};
pub use validator::{ConfigValidator, format_validation_errors};
