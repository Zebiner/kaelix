//! Configuration management for MemoryStreamer
//!
//! This module provides comprehensive configuration management with:
//! - Schema-driven configuration with validation
//! - Multi-source loading (files, environment, CLI overrides)
//! - Hot-reloading with file system monitoring
//! - Environment-specific configuration profiles
//! - Comprehensive validation and error reporting
//!
//! # Examples
//!
//! ## Basic configuration loading:
//!
//! ```rust
//! use kaelix_core::config::{ConfigLoader, MemoryStreamerConfig};
//!
//! let config = ConfigLoader::new()
//!     .load()
//!     .expect("Failed to load configuration");
//!
//! println!("Server will bind to {}:{}", config.network.bind_address, config.network.port);
//! ```
//!
//! ## Configuration with overrides:
//!
//! ```rust
//! use kaelix_core::config::{ConfigLoader, MemoryStreamerConfig};
//!
//! let config = ConfigLoader::new()
//!     .with_override("network.port", 8080)
//!     .with_override("performance.worker_threads", 16)
//!     .load()
//!     .expect("Failed to load configuration");
//! ```
//!
//! ## Hot-reloading configuration:
//!
//! ```rust
//! use kaelix_core::config::{HotReloadManager, ConfigLoader};
//! use std::path::Path;
//!
//! let config_path = Path::new("config.toml");
//! let mut hot_reload = HotReloadManager::new(config_path)
//!     .expect("Failed to create hot reload manager");
//!
//! let config = hot_reload.current_config();
//! ```

pub mod hot_reload;
pub mod loader;
pub mod schema;
pub mod validator;

// Re-export main types for convenience
pub use hot_reload::{
    ChangeType, ConfigChangeEvent, HotReloadManager, HotReloadSettings, HotReloadUtils, ReloadStats,
};
pub use loader::ConfigLoader;
pub use schema::*;
pub use validator::{ConfigValidator, ValidationContext};

use std::path::Path;

/// Load configuration from the default search paths
///
/// This is a convenience function that creates a ConfigLoader with default settings
/// and loads configuration from the standard search paths.
///
/// # Returns
/// * `Ok(MemoryStreamerConfig)` - Successfully loaded configuration
/// * `Err(Error)` - Configuration loading failed
///
/// # Examples
///
/// ```rust
/// use kaelix_core::config;
///
/// let config = config::load_default()
///     .expect("Failed to load default configuration");
/// ```
pub fn load_default() -> Result<MemoryStreamerConfig, std::io::Error> {
    ConfigLoader::new().load()
}

/// Load configuration from a specific file
///
/// # Arguments
/// * `path` - Path to the configuration file
///
/// # Returns
/// * `Ok(MemoryStreamerConfig)` - Successfully loaded configuration
/// * `Err(Error)` - Configuration loading failed
///
/// # Examples
///
/// ```rust
/// use kaelix_core::config;
/// use std::path::Path;
///
/// let config = config::load_from_file(Path::new("config.toml"))
///     .expect("Failed to load configuration file");
/// ```
pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<MemoryStreamerConfig, std::io::Error> {
    ConfigLoader::load_from_file(path)
}

/// Validate a configuration object
///
/// # Arguments
/// * `config` - Configuration to validate
///
/// # Returns
/// * `Ok(())` - Configuration is valid
/// * `Err(Error)` - Validation failed
///
/// # Examples
///
/// ```rust
/// use kaelix_core::config::{self, MemoryStreamerConfig};
///
/// let config = MemoryStreamerConfig::default();
/// config::validate(&config)
///     .expect("Configuration validation failed");
/// ```
pub fn validate(config: &MemoryStreamerConfig) -> Result<()> {
    let validator = ConfigValidator::new();
    validator.validate(config)
}

/// Create a configuration builder for programmatic configuration
///
/// This function returns a default configuration that can be modified programmatically
/// before validation.
///
/// # Returns
/// * `MemoryStreamerConfig` - Default configuration
///
/// # Examples
///
/// ```rust
/// use kaelix_core::config;
///
/// let mut config = config::builder();
/// config.network.port = 8080;
/// config.performance.worker_threads = 16;
///
/// config::validate(&config)
///     .expect("Configuration validation failed");
/// ```
pub fn builder() -> MemoryStreamerConfig {
    MemoryStreamerConfig::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_default_config() {
        let config = load_default().expect("Failed to load default configuration");
        
        // Verify some basic configuration values
        assert_eq!(config.network.port, 9092);
        assert!(config.performance.worker_threads > 0);
        assert!(!config.network.bind_address.is_empty());
    }

    #[test]
    fn test_validate_default_config() {
        let config = MemoryStreamerConfig::default();
        validate(&config).expect("Default configuration should be valid");
    }

    #[test]
    fn test_builder() {
        let mut config = builder();
        config.network.port = 8080;
        config.performance.worker_threads = 16;
        
        validate(&config).expect("Built configuration should be valid");
        assert_eq!(config.network.port, 8080);
        assert_eq!(config.performance.worker_threads, 16);
    }
}