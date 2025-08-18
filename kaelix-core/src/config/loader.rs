//! Configuration loading with multi-source support
//!
//! This module provides a sophisticated configuration loading system that supports:
//! - Default embedded configuration
//! - TOML file loading with path resolution
//! - Environment variable overrides
//! - Runtime configuration updates
//! - Comprehensive validation and error reporting
//!
//! The loader follows a layered approach where each source can override settings
//! from previous sources, allowing flexible deployment configurations.

use crate::config::schema::MemoryStreamerConfig;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use validator::Validate;

/// Configuration loader with multi-source support
///
/// The loader applies configuration sources in the following order:
/// 1. Default configuration (embedded in code)
/// 2. Configuration file (TOML format)
/// 3. Environment variable overrides
/// 4. Runtime overrides (programmatic)
///
/// Each layer can override settings from previous layers, providing
/// flexible configuration management for different deployment scenarios.
#[derive(Debug)]
pub struct ConfigLoader {
    /// Search paths for configuration files
    search_paths: Vec<PathBuf>,
    /// Environment variable prefix for overrides
    env_prefix: String,
    /// Runtime override values
    overrides: HashMap<String, toml::Value>,
}

impl ConfigLoader {
    /// Create a new configuration loader with default settings
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_core::config::ConfigLoader;
    ///
    /// let loader = ConfigLoader::new();
    /// let config = loader.load().expect("Failed to load configuration");
    /// ```
    pub fn new() -> Self {
        Self {
            search_paths: Self::default_search_paths(),
            env_prefix: "MEMORY_STREAMER".to_string(),
            overrides: HashMap::new(),
        }
    }

    /// Create a configuration loader with custom search paths
    ///
    /// # Arguments
    /// * `search_paths` - List of directories to search for configuration files
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_core::config::ConfigLoader;
    /// use std::path::PathBuf;
    ///
    /// let paths = vec![
    ///     PathBuf::from("/etc/memory-streamer"),
    ///     PathBuf::from("./config"),
    /// ];
    /// let loader = ConfigLoader::with_search_paths(paths);
    /// ```
    pub fn with_search_paths(search_paths: Vec<PathBuf>) -> Self {
        Self {
            search_paths,
            env_prefix: "MEMORY_STREAMER".to_string(),
            overrides: HashMap::new(),
        }
    }

    /// Set the environment variable prefix for configuration overrides
    ///
    /// # Arguments
    /// * `prefix` - Environment variable prefix (e.g., "MEMORY_STREAMER")
    ///
    /// Environment variables are mapped to configuration keys using dot notation:
    /// - `MEMORY_STREAMER_NETWORK_PORT=8080` sets `network.port = 8080`
    /// - `MEMORY_STREAMER_PERFORMANCE_WORKER_THREADS=8` sets `performance.worker_threads = 8`
    pub fn with_env_prefix<S: Into<String>>(mut self, prefix: S) -> Self {
        self.env_prefix = prefix.into();
        self
    }

    /// Add a runtime override for a specific configuration key
    ///
    /// # Arguments
    /// * `key` - Configuration key using dot notation (e.g., "network.port")
    /// * `value` - Value to override (must be serializable to TOML)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_core::config::ConfigLoader;
    ///
    /// let loader = ConfigLoader::new()
    ///     .with_override("network.port", 9090)
    ///     .with_override("performance.worker_threads", 16);
    /// ```
    pub fn with_override<T: Into<toml::Value>>(mut self, key: &str, value: T) -> Self {
        self.overrides.insert(key.to_string(), value.into());
        self
    }

    /// Load configuration from all sources
    ///
    /// This method applies the complete configuration loading pipeline:
    /// 1. Start with default configuration
    /// 2. Load and merge configuration file if found
    /// 3. Apply environment variable overrides
    /// 4. Apply runtime overrides
    /// 5. Validate the final configuration
    ///
    /// # Returns
    /// * `Ok(MemoryStreamerConfig)` - Successfully loaded and validated configuration
    /// * `Err(Error)` - Configuration loading or validation failed
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_core::config::ConfigLoader;
    ///
    /// let config = ConfigLoader::new()
    ///     .load()
    ///     .expect("Failed to load configuration");
    ///
    /// println!("Server will bind to {}:{}", config.network.bind_address, config.network.port);
    /// ```
    pub fn load(&self) -> Result<MemoryStreamerConfig> {
        info!("Loading configuration from multiple sources");

        // Start with default configuration
        let mut config_value = toml::Value::try_from(MemoryStreamerConfig::default())
            .map_err(|e| Error::Configuration { 
                message: format!("Failed to serialize default config: {}", e)
            })?;

        debug!("Applied default configuration");

        // Load configuration file if found
        if let Some(config_path) = self.find_config_file()? {
            let file_config = self.load_config_file(&config_path)?;
            config_value = self.merge_config(config_value, file_config)?;
            info!("Loaded configuration file: {}", config_path.display());
        } else {
            debug!("No configuration file found in search paths");
        }

        // Apply environment variable overrides
        let env_overrides = self.load_env_overrides()?;
        if self.has_meaningful_config(&env_overrides) {
            config_value = self.merge_config(config_value, env_overrides)?;
            debug!("Applied environment variable overrides");
        }

        // Apply runtime overrides
        if !self.overrides.is_empty() {
            let runtime_overrides = self.build_nested_config(&self.overrides)?;
            config_value = self.merge_config(config_value, runtime_overrides)?;
            debug!("Applied runtime overrides");
        }

        // Convert to final configuration structure
        let config: MemoryStreamerConfig = config_value.try_into()
            .map_err(|e| Error::Configuration {
                message: format!("Failed to deserialize config: {}", e)
            })?;

        // Validate the configuration
        config.validate()
            .map_err(|e| Error::Configuration {
                message: format!("Configuration validation failed: {}", e)
            })?;

        info!("Configuration loaded and validated successfully");
        Ok(config)
    }

    /// Load configuration from a specific file
    ///
    /// # Arguments
    /// * `path` - Path to the configuration file
    ///
    /// # Returns
    /// * `Ok(MemoryStreamerConfig)` - Successfully loaded configuration
    /// * `Err(Error)` - File loading or parsing failed
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<MemoryStreamerConfig> {
        let path = path.as_ref();
        info!("Loading configuration from file: {}", path.display());

        let content = fs::read_to_string(path)
            .map_err(|e| Error::Configuration {
                message: format!("Failed to read config file '{}': {}", path.display(), e)
            })?;

        let config: MemoryStreamerConfig = toml::from_str(&content)
            .map_err(|e| Error::Configuration {
                message: format!("Failed to parse config file '{}': {}", path.display(), e)
            })?;

        config.validate()
            .map_err(|e| Error::Configuration {
                message: format!("Configuration validation failed for '{}': {}", path.display(), e)
            })?;

        info!("Configuration loaded successfully from file: {}", path.display());
        Ok(config)
    }

    /// Check if a toml::Value contains meaningful configuration
    fn has_meaningful_config(&self, value: &toml::Value) -> bool {
        match value {
            toml::Value::Table(table) => !table.is_empty(),
            _ => true,
        }
    }

    /// Get default search paths for configuration files
    ///
    /// The search order is:
    /// 1. Current directory (./memory-streamer.toml)
    /// 2. User config directory (~/.config/memory-streamer/config.toml)
    /// 3. System config directory (/etc/memory-streamer/config.toml)
    /// 4. Application directory (relative to executable)
    fn default_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Current directory
        paths.push(PathBuf::from("./memory-streamer.toml"));
        paths.push(PathBuf::from("./config.toml"));

        // User config directory
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("memory-streamer").join("config.toml"));
        }

        // System config directory
        paths.push(PathBuf::from("/etc/memory-streamer/config.toml"));

        // Application directory (relative to executable)
        if let Ok(exe_path) = env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                paths.push(exe_dir.join("config.toml"));
                paths.push(exe_dir.join("memory-streamer.toml"));
            }
        }

        paths
    }

    /// Find the first configuration file that exists in the search paths
    fn find_config_file(&self) -> Result<Option<PathBuf>> {
        for path in &self.search_paths {
            if path.exists() && path.is_file() {
                debug!("Found configuration file: {}", path.display());
                return Ok(Some(path.clone()));
            }
        }
        Ok(None)
    }

    /// Load configuration from a TOML file
    fn load_config_file(&self, path: &Path) -> Result<toml::Value> {
        let content = fs::read_to_string(path)
            .map_err(|e| Error::Configuration {
                message: format!("Failed to read config file '{}': {}", path.display(), e)
            })?;

        let config: toml::Value = toml::from_str(&content)
            .map_err(|e| Error::Configuration {
                message: format!("Failed to parse config file '{}': {}", path.display(), e)
            })?;

        Ok(config)
    }

    /// Load environment variable overrides
    fn load_env_overrides(&self) -> Result<toml::Value> {
        let mut overrides = HashMap::new();
        let prefix = format!("{}_", self.env_prefix);

        for (key, value) in env::vars() {
            if key.starts_with(&prefix) {
                let config_key = key[prefix.len()..]
                    .to_lowercase()
                    .replace('_', ".");

                // Try to parse as different types
                let toml_value = if let Ok(int_val) = value.parse::<i64>() {
                    toml::Value::Integer(int_val)
                } else if let Ok(float_val) = value.parse::<f64>() {
                    toml::Value::Float(float_val)
                } else if let Ok(bool_val) = value.parse::<bool>() {
                    toml::Value::Boolean(bool_val)
                } else {
                    toml::Value::String(value)
                };

                debug!("Applied environment override: {} = {:?}", key, &toml_value);
                overrides.insert(config_key, toml_value);
            }
        }

        self.build_nested_config(&overrides)
    }

    /// Build nested configuration structure from flat key-value pairs
    fn build_nested_config(&self, overrides: &HashMap<String, toml::Value>) -> Result<toml::Value> {
        let mut config = toml::Value::Table(toml::map::Map::new());

        for (key, value) in overrides {
            self.set_nested_value(&mut config, key, value.clone())?;
        }

        Ok(config)
    }

    /// Set a nested value in a TOML structure using dot notation
    fn set_nested_value(&self, config: &mut toml::Value, key: &str, value: toml::Value) -> Result<()> {
        let parts: Vec<&str> = key.split('.').collect();
        
        if parts.is_empty() {
            return Err(Error::Configuration {
                message: "Empty configuration key".to_string()
            });
        }

        let mut current = config;
        
        // Navigate to the parent of the target key
        for part in &parts[..parts.len() - 1] {
            if let toml::Value::Table(table) = current {
                current = table.entry(part.to_string())
                    .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            } else {
                return Err(Error::Configuration {
                    message: format!("Cannot set nested value '{}': parent is not a table", key)
                });
            }
        }

        // Set the final value
        if let toml::Value::Table(table) = current {
            table.insert(parts[parts.len() - 1].to_string(), value);
        } else {
            return Err(Error::Configuration {
                message: format!("Cannot set value '{}': parent is not a table", key)
            });
        }

        Ok(())
    }

    /// Merge two TOML configuration values
    fn merge_config(&self, mut base: toml::Value, override_value: toml::Value) -> Result<toml::Value> {
        match (&mut base, override_value) {
            (toml::Value::Table(base_table), toml::Value::Table(override_table)) => {
                for (key, value) in override_table {
                    if let Some(base_value) = base_table.get_mut(&key) {
                        *base_value = self.merge_config(base_value.clone(), value)?;
                    } else {
                        base_table.insert(key, value);
                    }
                }
                Ok(base)
            }
            (_, override_value) => Ok(override_value),
        }
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_config_loader_new() {
        let loader = ConfigLoader::new();
        assert_eq!(loader.env_prefix, "MEMORY_STREAMER");
        assert!(!loader.search_paths.is_empty());
        assert!(loader.overrides.is_empty());
    }

    #[test]
    fn test_config_loader_with_env_prefix() {
        let loader = ConfigLoader::new().with_env_prefix("CUSTOM_PREFIX");
        assert_eq!(loader.env_prefix, "CUSTOM_PREFIX");
    }

    #[test]
    fn test_config_loader_with_override() {
        let loader = ConfigLoader::new()
            .with_override("network.port", 9090)
            .with_override("performance.worker_threads", 16);
        
        assert_eq!(loader.overrides.len(), 2);
        assert_eq!(loader.overrides.get("network.port").unwrap(), &toml::Value::Integer(9090));
        assert_eq!(loader.overrides.get("performance.worker_threads").unwrap(), &toml::Value::Integer(16));
    }

    #[test]
    fn test_load_default_config() {
        let loader = ConfigLoader::new();
        let config = loader.load().expect("Failed to load default configuration");
        
        // Verify default values are loaded
        assert_eq!(config.network.port, 9092);
        assert!(config.performance.worker_threads.is_some());
    }

    #[test]
    fn test_load_from_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        
        let config_content = r#"
[network]
port = 8080
bind_address = "127.0.0.1"

[performance]
worker_threads = 4
        "#;
        
        temp_file.write_all(config_content.as_bytes()).expect("Failed to write temp file");
        
        let config = ConfigLoader::load_from_file(temp_file.path())
            .expect("Failed to load config from file");
        
        assert_eq!(config.network.port, 8080);
        assert_eq!(config.network.bind_address, "127.0.0.1");
        assert_eq!(config.performance.worker_threads, Some(4));
    }

    #[test]
    fn test_set_nested_value() {
        let loader = ConfigLoader::new();
        let mut config = toml::Value::Table(toml::map::Map::new());
        
        loader.set_nested_value(&mut config, "network.port", toml::Value::Integer(8080))
            .expect("Failed to set nested value");
        
        if let toml::Value::Table(table) = &config {
            if let Some(toml::Value::Table(network_table)) = table.get("network") {
                assert_eq!(network_table.get("port").unwrap(), &toml::Value::Integer(8080));
            } else {
                panic!("Network table not found");
            }
        } else {
            panic!("Config is not a table");
        }
    }

    #[test]
    fn test_merge_config() {
        let loader = ConfigLoader::new();
        
        let base = toml::from_str(r#"
[network]
port = 9092
bind_address = "0.0.0.0"

[performance]
worker_threads = 8
        "#).unwrap();
        
        let override_config = toml::from_str(r#"
[network]
port = 8080

[performance]
buffer_size = 1024
        "#).unwrap();
        
        let merged = loader.merge_config(base, override_config)
            .expect("Failed to merge config");
        
        let config: MemoryStreamerConfig = merged.try_into()
            .expect("Failed to convert merged config");
        
        assert_eq!(config.network.port, 8080);  // Overridden
        assert_eq!(config.network.bind_address, "0.0.0.0");  // Original
        assert_eq!(config.performance.worker_threads, Some(8));  // Original
    }

    #[test]
    fn test_has_meaningful_config() {
        let loader = ConfigLoader::new();
        
        // Empty table should be false
        let empty_table = toml::Value::Table(toml::map::Map::new());
        assert!(!loader.has_meaningful_config(&empty_table));
        
        // Non-empty table should be true
        let mut non_empty_table = toml::map::Map::new();
        non_empty_table.insert("key".to_string(), toml::Value::String("value".to_string()));
        let non_empty = toml::Value::Table(non_empty_table);
        assert!(loader.has_meaningful_config(&non_empty));
        
        // Other values should be true
        let string_value = toml::Value::String("test".to_string());
        assert!(loader.has_meaningful_config(&string_value));
    }
}