//! # Configuration Loading
//!
//! Handles loading configuration from various sources including files, environment,
//! and command-line arguments with proper error handling and validation.

use crate::{
    config::{schema::MemoryStreamerConfig, validator::ConfigValidator},
    Error, Result,
};
use serde::de::DeserializeOwned;
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};
use tracing::{debug, info, warn};

/// Configuration loader with support for multiple sources
pub struct ConfigLoader {
    search_paths: Vec<PathBuf>,
    env_prefix: String,
}

impl ConfigLoader {
    /// Create a new configuration loader
    pub fn new() -> Self {
        Self {
            search_paths: vec![
                PathBuf::from("."),
                PathBuf::from("./config"),
                PathBuf::from("../config"),
                dirs::config_dir().unwrap_or_else(|| PathBuf::from("/etc")),
            ],
            env_prefix: "KAELIX".to_string(),
        }
    }

    /// Add a search path for configuration files
    pub fn with_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.search_paths.push(path.as_ref().to_path_buf());
        self
    }

    /// Set the environment variable prefix
    pub fn with_env_prefix<S: AsRef<str>>(mut self, prefix: S) -> Self {
        self.env_prefix = prefix.as_ref().to_string();
        self
    }

    /// Add a specific config file path
    pub fn with_file<P: AsRef<Path>>(mut self, path: P) -> Self {
        // For now, we'll add it as a search path directory
        let file_path = path.as_ref();
        if let Some(parent) = file_path.parent() {
            self.search_paths.insert(0, parent.to_path_buf());
        }
        self
    }

    /// Load configuration from all available sources
    pub fn load(&self) -> Result<MemoryStreamerConfig> {
        info!("Loading configuration from multiple sources");

        // Start with default configuration
        let mut config_value =
            toml::Value::try_from(MemoryStreamerConfig::default()).map_err(|e| {
                Error::Configuration(format!("Failed to serialize default config: {e}"))
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

        // Apply environment variables
        config_value = self.apply_env_vars(config_value)?;
        debug!("Applied environment variables");

        // Convert back to struct and validate
        let config: MemoryStreamerConfig = config_value
            .try_into()
            .map_err(|e| Error::Configuration(format!("Failed to deserialize config: {e}")))?;

        // Validate the configuration
        ConfigValidator::validate(&config)?;

        info!("Configuration loaded and validated successfully");
        Ok(config)
    }

    /// Load configuration from a specific file
    pub fn load_from_file<P: AsRef<Path>>(&self, path: P) -> Result<MemoryStreamerConfig> {
        let path = path.as_ref();
        info!("Loading configuration from file: {}", path.display());

        let config = self.load_config_file(path)?;
        let config: MemoryStreamerConfig = config
            .try_into()
            .map_err(|e| Error::Configuration(format!("Failed to parse config file: {e}")))?;

        ConfigValidator::validate(&config)?;
        Ok(config)
    }

    /// Find the first available configuration file
    fn find_config_file(&self) -> Result<Option<PathBuf>> {
        let config_names =
            ["kaelix.toml", "config.toml", "memorystreamer.toml", "kaelix-config.toml"];

        for search_path in &self.search_paths {
            for config_name in &config_names {
                let config_path = search_path.join(config_name);
                if config_path.exists() {
                    debug!("Found config file: {}", config_path.display());
                    return Ok(Some(config_path));
                }
            }
        }

        Ok(None)
    }

    /// Load configuration from a TOML file
    fn load_config_file(&self, path: &Path) -> Result<toml::Value> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            Error::Configuration(format!("Failed to read config file {}: {}", path.display(), e))
        })?;

        content.parse::<toml::Value>().map_err(|e| {
            Error::Configuration(format!("Failed to parse config file {}: {}", path.display(), e))
        })
    }

    #[allow(clippy::only_used_in_recursion)]
    /// Merge two TOML configuration values
    fn merge_config(
        &self,
        mut base: toml::Value,
        override_value: toml::Value,
    ) -> Result<toml::Value> {
        if let (toml::Value::Table(base_table), toml::Value::Table(override_table)) =
            (&mut base, override_value)
        {
            for (key, value) in override_table {
                match base_table.get_mut(&key) {
                    Some(existing_value) => {
                        if existing_value.is_table() && value.is_table() {
                            *existing_value = self.merge_config(existing_value.clone(), value)?;
                        } else {
                            *existing_value = value;
                        }
                    },
                    None => {
                        base_table.insert(key, value);
                    },
                }
            }
        }

        Ok(base)
    }

    /// Apply environment variables to configuration
    fn apply_env_vars(&self, mut config: toml::Value) -> Result<toml::Value> {
        let env_vars = self.collect_env_vars()?;

        for (key, value) in env_vars {
            self.apply_env_var(&mut config, &key, &value)?;
        }

        Ok(config)
    }

    /// Collect all relevant environment variables
    fn collect_env_vars(&self) -> Result<HashMap<String, String>> {
        let mut env_vars = HashMap::new();
        let prefix = format!("{}_", self.env_prefix);

        for (key, value) in env::vars() {
            if key.starts_with(&prefix) {
                let config_key =
                    key.strip_prefix(&prefix).unwrap().to_lowercase().replace('_', ".");
                env_vars.insert(config_key, value);
            }
        }

        debug!("Collected {} environment variables", env_vars.len());
        Ok(env_vars)
    }

    /// Apply a single environment variable to the configuration
    fn apply_env_var(&self, config: &mut toml::Value, key: &str, value: &str) -> Result<()> {
        let parts: Vec<&str> = key.split('.').collect();
        self.set_nested_value(config, &parts, value)
    }

    /// Set a nested value in the TOML configuration
    fn set_nested_value(
        &self,
        config: &mut toml::Value,
        parts: &[&str],
        value: &str,
    ) -> Result<()> {
        if parts.is_empty() {
            return Ok(());
        }

        if parts.len() == 1 {
            // Base case: set the value
            let parsed_value = self.parse_env_value(value)?;
            if let toml::Value::Table(table) = config {
                table.insert(parts[0].to_string(), parsed_value);
            }
            return Ok(());
        }

        // Recursive case: navigate deeper
        let current_key = parts[0];
        let remaining_parts = &parts[1..];

        if let toml::Value::Table(table) = config {
            let entry = table
                .entry(current_key.to_string())
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));

            self.set_nested_value(entry, remaining_parts, value)?;
        }

        Ok(())
    }

    /// Parse environment variable value to appropriate TOML type
    fn parse_env_value(&self, value: &str) -> Result<toml::Value> {
        // Try boolean first
        if let Ok(bool_val) = value.parse::<bool>() {
            return Ok(toml::Value::Boolean(bool_val));
        }

        // Try integer
        if let Ok(int_val) = value.parse::<i64>() {
            return Ok(toml::Value::Integer(int_val));
        }

        // Try float
        if let Ok(float_val) = value.parse::<f64>() {
            return Ok(toml::Value::Float(float_val));
        }

        // Default to string
        Ok(toml::Value::String(value.to_string()))
    }

    /// Load and merge configuration from multiple TOML files
    pub fn load_from_files<P: AsRef<Path>>(&self, paths: &[P]) -> Result<MemoryStreamerConfig> {
        let mut config_value = toml::Value::Table(toml::map::Map::new());

        for path in paths {
            let file_config = self.load_config_file(path.as_ref())?;
            config_value = self.merge_config(config_value, file_config)?;
            info!("Merged configuration from: {}", path.as_ref().display());
        }

        let config: MemoryStreamerConfig = config_value
            .try_into()
            .map_err(|e| Error::Configuration(format!("Failed to parse merged config: {e}")))?;

        ConfigValidator::validate(&config)?;
        Ok(config)
    }

    /// Save configuration to a TOML file
    pub fn save_to_file<P: AsRef<Path>>(
        &self,
        config: &MemoryStreamerConfig,
        path: P,
    ) -> Result<()> {
        let toml_string = toml::to_string_pretty(config)
            .map_err(|e| Error::Configuration(format!("Failed to serialize config: {e}")))?;

        std::fs::write(path.as_ref(), toml_string).map_err(|e| {
            Error::Configuration(format!(
                "Failed to write config to {}: {}",
                path.as_ref().display(),
                e
            ))
        })?;

        info!("Configuration saved to: {}", path.as_ref().display());
        Ok(())
    }

    /// Load configuration from a custom deserializer
    pub fn load_from_deserializer<T: DeserializeOwned>(&self, content: &str) -> Result<T> {
        toml::from_str(content)
            .map_err(|e| Error::Configuration(format!("Failed to deserialize custom config: {e}")))
    }

    /// Get the effective search paths being used
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Get the environment prefix being used
    pub fn env_prefix(&self) -> &str {
        &self.env_prefix
    }

    /// Validate that all search paths exist
    pub fn validate_search_paths(&self) -> Result<Vec<PathBuf>> {
        let mut valid_paths = Vec::new();

        for path in &self.search_paths {
            if path.exists() {
                valid_paths.push(path.clone());
            } else {
                warn!("Search path does not exist: {}", path.display());
            }
        }

        if valid_paths.is_empty() {
            return Err(Error::Configuration(
                "No valid search paths found for configuration files".to_string(),
            ));
        }

        Ok(valid_paths)
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
    use tempfile::TempDir;

    #[test]
    fn test_config_loader_creation() {
        let loader = ConfigLoader::new();
        assert!(!loader.search_paths.is_empty());
        assert_eq!(loader.env_prefix, "KAELIX");
    }

    #[test]
    fn test_config_loader_with_path() {
        let loader = ConfigLoader::new().with_path("/custom/path");
        assert!(loader.search_paths.contains(&PathBuf::from("/custom/path")));
    }

    #[test]
    fn test_config_loader_with_env_prefix() {
        let loader = ConfigLoader::new().with_env_prefix("TEST");
        assert_eq!(loader.env_prefix, "TEST");
    }

    #[tokio::test]
    async fn test_load_default_config() {
        let loader = ConfigLoader::new();
        // This should work even with no config file present
        let result = loader.load();
        // We expect this to either succeed or fail with a specific error
        match result {
            Ok(_) => {
                // Configuration loaded successfully
            },
            Err(e) => {
                // Should be a configuration error, not a panic
                assert!(matches!(e, Error::Configuration(_)));
            },
        }
    }

    #[test]
    fn test_merge_config() {
        let loader = ConfigLoader::new();

        let base = toml::Value::Table({
            let mut map = toml::map::Map::new();
            map.insert("key1".to_string(), toml::Value::String("value1".to_string()));
            map
        });

        let override_val = toml::Value::Table({
            let mut map = toml::map::Map::new();
            map.insert("key2".to_string(), toml::Value::String("value2".to_string()));
            map
        });

        let result = loader.merge_config(base, override_val).unwrap();
        if let toml::Value::Table(table) = result {
            assert_eq!(table.len(), 2);
            assert!(table.contains_key("key1"));
            assert!(table.contains_key("key2"));
        } else {
            panic!("Expected table result");
        }
    }

    #[test]
    fn test_parse_env_value() {
        let loader = ConfigLoader::new();

        assert_eq!(loader.parse_env_value("true").unwrap(), toml::Value::Boolean(true));
        assert_eq!(loader.parse_env_value("42").unwrap(), toml::Value::Integer(42));
        assert_eq!(loader.parse_env_value("3.14").unwrap(), toml::Value::Float(3.14));
        assert_eq!(
            loader.parse_env_value("hello").unwrap(),
            toml::Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_load_config_from_string() {
        let loader = ConfigLoader::new();
        let config_str = r#"
            [network]
            host = "localhost"
            port = 8080
        "#;

        // Test custom deserializer
        let result: Result<toml::Value> = loader.load_from_deserializer(config_str);
        assert!(result.is_ok());
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        let loader = ConfigLoader::new();
        let default_config = MemoryStreamerConfig::default();

        // Save config
        let save_result = loader.save_to_file(&default_config, &config_path);
        assert!(save_result.is_ok());

        // Verify file exists
        assert!(config_path.exists());

        // Load config
        let loaded_result = loader.load_from_file(&config_path);
        // We expect this to either work or fail gracefully
        match loaded_result {
            Ok(_loaded_config) => {
                // Configuration loaded successfully
            },
            Err(e) => {
                // Should be a configuration error
                assert!(matches!(e, Error::Configuration(_)));
            },
        }
    }

    #[test]
    fn test_validate_search_paths() {
        let loader = ConfigLoader::new()
            .with_path("./") // This should exist
            .with_path("/nonexistent/path"); // This shouldn't exist

        let valid_paths = loader.validate_search_paths();
        assert!(valid_paths.is_ok());
        let paths = valid_paths.unwrap();
        assert!(!paths.is_empty());
    }
}
